//! Hyprland overlay: a `wlr-layer-shell` surface covering the output, with
//! an empty input region (clicks fall through) and `exclusive_zone = -1`
//! (does not shove windows). Draws one premultiplied-alpha quad in the
//! center — the first visible Presence pixels. Hyperbubble comes next.

use crate::pipeline::SplatPipeline;
use crate::socket::SocketServer;
use crate::vulkan::VulkanContext;
use ash::{khr, vk};
use std::io;
use std::os::fd::{AsFd, AsRawFd};
use std::path::Path;
use wayland_client::protocol::{
    wl_compositor, wl_output, wl_region, wl_registry, wl_surface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};

const OVERLAY_VERT_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/overlay_quad.vert.spv"));
const OVERLAY_FRAG_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/overlay_quad.frag.spv"));

/// Visible marker size in pixels. Small enough not to dominate a 1080p
/// screen, large enough to prove the overlay is actually compositing.
const QUAD_PX: f32 = 200.0;

pub struct Overlay {
    conn: Connection,
    event_queue: EventQueue<OverlayState>,
    state: OverlayState,
}

pub struct OverlayState {
    compositor: Option<wl_compositor::WlCompositor>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    surface: Option<wl_surface::WlSurface>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    width: u32,
    height: u32,
    configured: bool,
    closed: bool,
    dirty: bool,
}

impl Overlay {
    pub fn connect() -> Result<Self, String> {
        let conn = Connection::connect_to_env()
            .map_err(|e| format!("no Wayland compositor (WAYLAND_DISPLAY?): {e}"))?;
        let mut event_queue = conn.new_event_queue();
        let qh = event_queue.handle();
        let display = conn.display();
        let _registry = display.get_registry(&qh, ());

        let mut state = OverlayState {
            compositor: None,
            layer_shell: None,
            surface: None,
            layer_surface: None,
            width: 0,
            height: 0,
            configured: false,
            closed: false,
            dirty: false,
        };
        event_queue
            .roundtrip(&mut state)
            .map_err(|e| format!("wayland registry roundtrip: {e}"))?;

        let compositor = state
            .compositor
            .clone()
            .ok_or("compositor did not advertise wl_compositor")?;
        let layer_shell = state
            .layer_shell
            .clone()
            .ok_or("compositor did not advertise zwlr_layer_shell_v1 (need Hyprland / a wlr compositor)")?;

        let surface = compositor.create_surface(&qh, ());
        // Empty region: no input (pass-through) and no opaque pixels
        // (compositor alpha-blends the whole surface).
        let region = compositor.create_region(&qh, ());
        surface.set_input_region(Some(&region));
        surface.set_opaque_region(Some(&region));

        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Overlay,
            "pachakutech-presence".into(),
            &qh,
            (),
        );
        layer_surface.set_anchor(
            zwlr_layer_surface_v1::Anchor::Top
                | zwlr_layer_surface_v1::Anchor::Bottom
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        );
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_keyboard_interactivity(
            zwlr_layer_surface_v1::KeyboardInteractivity::None,
        );
        layer_surface.set_size(0, 0);
        surface.commit();

        state.surface = Some(surface);
        state.layer_surface = Some(layer_surface);

        // Block until the compositor tells us our size. First commit must
        // have no buffer; configure comes back, then we may attach Vulkan.
        for _ in 0..16 {
            event_queue
                .roundtrip(&mut state)
                .map_err(|e| format!("wayland configure roundtrip: {e}"))?;
            if state.configured && state.width > 0 && state.height > 0 {
                break;
            }
        }
        if !state.configured || state.width == 0 || state.height == 0 {
            return Err(
                "layer-shell never configured a non-zero size — compositor refused the overlay"
                    .into(),
            );
        }

        println!(
            "[overlay] layer-shell {}x{} exclusive_zone=-1 input=pass-through",
            state.width, state.height
        );

        Ok(Self { conn, event_queue, state })
    }

    pub fn display_ptr(&self) -> *mut vk::wl_display {
        self.conn.backend().display_ptr() as *mut vk::wl_display
    }

    pub fn surface_ptr(&self) -> Result<*mut vk::wl_surface, String> {
        let surface = self.state.surface.as_ref().ok_or("overlay has no wl_surface")?;
        let ptr = surface.id().as_ptr();
        if ptr.is_null() {
            return Err("wl_surface proxy was destroyed".into());
        }
        Ok(ptr as *mut vk::wl_surface)
    }

    pub fn extent(&self) -> vk::Extent2D {
        vk::Extent2D { width: self.state.width, height: self.state.height }
    }

    pub fn closed(&self) -> bool {
        self.state.closed
    }

    pub fn take_dirty(&mut self) -> bool {
        let d = self.state.dirty;
        self.state.dirty = false;
        d
    }

    fn flush_and_dispatch(&mut self) -> Result<(), String> {
        self.event_queue
            .flush()
            .map_err(|e| format!("wayland flush: {e}"))?;
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| format!("wayland dispatch: {e}"))?;
        Ok(())
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for OverlayState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_compositor" if state.compositor.is_none() => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "zwlr_layer_shell_v1" if state.layer_shell.is_none() => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for OverlayState {
    fn event(
        state: &mut Self,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                layer_surface.ack_configure(serial);
                if width > 0 {
                    state.width = width;
                }
                if height > 0 {
                    state.height = height;
                }
                state.configured = true;
                state.dirty = true;
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state.closed = true;
            }
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(OverlayState: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(OverlayState: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(OverlayState: ignore wl_region::WlRegion);
wayland_client::delegate_noop!(OverlayState: ignore wl_output::WlOutput);
wayland_client::delegate_noop!(OverlayState: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

/// Swapchain + a one-quad graphics pipeline targeting the layer-shell surface.
pub struct OverlayGpu {
    swapchain_loader: khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    image_views: Vec<vk::ImageView>,
    format: vk::Format,
    extent: vk::Extent2D,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    in_flight: vk::Fence,
}

impl OverlayGpu {
    pub fn new(vk: &VulkanContext, extent: vk::Extent2D) -> Result<Self, String> {
        let surface = vk.surface.ok_or("VulkanContext has no Wayland VkSurface")?;
        let surface_loader = vk.surface_loader.as_ref().ok_or("no KHR_surface loader")?;
        let swapchain_loader = khr::swapchain::Device::new(&vk.instance, &vk.device);

        let (swapchain, format, extent, images) =
            create_swapchain(vk, &swapchain_loader, surface_loader, surface, extent, vk::SwapchainKHR::null())?;

        let image_views = create_image_views(&vk.device, format, &images)?;
        let render_pass = create_render_pass(&vk.device, format)?;
        let framebuffers = create_framebuffers(&vk.device, render_pass, extent, &image_views)?;
        let (pipeline_layout, pipeline) = create_quad_pipeline(&vk.device, render_pass)?;

        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(vk.graphics_queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { vk.device.create_command_pool(&pool_info, None) }
            .map_err(|e| format!("vkCreateCommandPool (overlay) failed: {e:?}"))?;
        let alloc = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = unsafe { vk.device.allocate_command_buffers(&alloc) }
            .map_err(|e| format!("vkAllocateCommandBuffers (overlay) failed: {e:?}"))?[0];

        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let image_available = unsafe { vk.device.create_semaphore(&semaphore_info, None) }
            .map_err(|e| format!("vkCreateSemaphore failed: {e:?}"))?;
        let render_finished = unsafe { vk.device.create_semaphore(&semaphore_info, None) }
            .map_err(|e| format!("vkCreateSemaphore failed: {e:?}"))?;
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let in_flight = unsafe { vk.device.create_fence(&fence_info, None) }
            .map_err(|e| format!("vkCreateFence (overlay) failed: {e:?}"))?;

        Ok(Self {
            swapchain_loader,
            swapchain,
            image_views,
            format,
            extent,
            render_pass,
            framebuffers,
            pipeline_layout,
            pipeline,
            command_pool,
            command_buffer,
            image_available,
            render_finished,
            in_flight,
        })
    }

    pub fn recreate(&mut self, vk: &VulkanContext, extent: vk::Extent2D) -> Result<(), String> {
        unsafe { vk.device.device_wait_idle() }.ok();
        self.destroy_swapchain_resources(&vk.device);
        let surface = vk.surface.unwrap();
        let surface_loader = vk.surface_loader.as_ref().unwrap();
        let (swapchain, format, extent, images) = create_swapchain(
            vk,
            &self.swapchain_loader,
            surface_loader,
            surface,
            extent,
            self.swapchain,
        )?;
        unsafe { self.swapchain_loader.destroy_swapchain(self.swapchain, None) };
        self.swapchain = swapchain;
        self.format = format;
        self.extent = extent;
        self.image_views = create_image_views(&vk.device, format, &images)?;
        self.render_pass = create_render_pass(&vk.device, format)?;
        self.framebuffers = create_framebuffers(&vk.device, self.render_pass, extent, &self.image_views)?;
        let (layout, pipeline) = create_quad_pipeline(&vk.device, self.render_pass)?;
        unsafe {
            vk.device.destroy_pipeline(self.pipeline, None);
            vk.device.destroy_pipeline_layout(self.pipeline_layout, None);
        }
        self.pipeline_layout = layout;
        self.pipeline = pipeline;
        Ok(())
    }

    pub fn draw(&mut self, vk: &VulkanContext) -> Result<(), String> {
        unsafe { vk.device.wait_for_fences(&[self.in_flight], true, u64::MAX) }
            .map_err(|e| format!("overlay fence wait: {e:?}"))?;

        let acquire = unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.image_available,
                vk::Fence::null(),
            )
        };
        let image_index = match acquire {
            Ok((idx, _)) => idx,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(()),
            Err(e) => return Err(format!("vkAcquireNextImageKHR failed: {e:?}")),
        };

        unsafe { vk.device.reset_fences(&[self.in_flight]) }
            .map_err(|e| format!("overlay reset fences: {e:?}"))?;
        unsafe { vk.device.reset_command_buffer(self.command_buffer, vk::CommandBufferResetFlags::empty()) }
            .map_err(|e| format!("overlay reset cmd: {e:?}"))?;

        let begin = vk::CommandBufferBeginInfo::default();
        unsafe { vk.device.begin_command_buffer(self.command_buffer, &begin) }
            .map_err(|e| format!("overlay begin cmd: {e:?}"))?;

        let clear = vk::ClearValue { color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 0.0] } };
        let render_area = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: self.extent };
        let rp_begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index as usize])
            .render_area(render_area)
            .clear_values(std::slice::from_ref(&clear));
        unsafe {
            vk.device.cmd_begin_render_pass(
                self.command_buffer,
                &rp_begin,
                vk::SubpassContents::INLINE,
            );
            vk.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            );
            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.extent.width as f32,
                height: self.extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };
            vk.device.cmd_set_viewport(self.command_buffer, 0, &[viewport]);
            vk.device.cmd_set_scissor(self.command_buffer, 0, &[render_area]);
            let pc = [self.extent.width as f32, self.extent.height as f32, QUAD_PX, QUAD_PX];
            vk.device.cmd_push_constants(
                self.command_buffer,
                self.pipeline_layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                std::slice::from_raw_parts(pc.as_ptr() as *const u8, 16),
            );
            vk.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
            vk.device.cmd_end_render_pass(self.command_buffer);
        }
        unsafe { vk.device.end_command_buffer(self.command_buffer) }
            .map_err(|e| format!("overlay end cmd: {e:?}"))?;

        let wait = [self.image_available];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let cmds = [self.command_buffer];
        let signal = [self.render_finished];
        let submit = vk::SubmitInfo::default()
            .wait_semaphores(&wait)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&cmds)
            .signal_semaphores(&signal);
        unsafe { vk.device.queue_submit(vk.graphics_queue, &[submit], self.in_flight) }
            .map_err(|e| format!("overlay queue_submit: {e:?}"))?;

        let swapchains = [self.swapchain];
        let indices = [image_index];
        let present = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal)
            .swapchains(&swapchains)
            .image_indices(&indices);
        match unsafe { self.swapchain_loader.queue_present(vk.graphics_queue, &present) } {
            Ok(_) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) | Err(vk::Result::SUBOPTIMAL_KHR) => Ok(()),
            Err(e) => Err(format!("vkQueuePresentKHR failed: {e:?}")),
        }
    }

    fn destroy_swapchain_resources(&mut self, device: &ash::Device) {
        unsafe {
            for fb in self.framebuffers.drain(..) {
                device.destroy_framebuffer(fb, None);
            }
            for view in self.image_views.drain(..) {
                device.destroy_image_view(view, None);
            }
            device.destroy_render_pass(self.render_pass, None);
        }
    }

    pub fn destroy(&mut self, vk: &VulkanContext) {
        unsafe {
            let _ = vk.device.device_wait_idle();
            self.destroy_swapchain_resources(&vk.device);
            vk.device.destroy_pipeline(self.pipeline, None);
            vk.device.destroy_pipeline_layout(self.pipeline_layout, None);
            vk.device.destroy_command_pool(self.command_pool, None);
            vk.device.destroy_semaphore(self.image_available, None);
            vk.device.destroy_semaphore(self.render_finished, None);
            vk.device.destroy_fence(self.in_flight, None);
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);
        }
    }
}

fn create_swapchain(
    vk: &VulkanContext,
    swapchain_loader: &khr::swapchain::Device,
    surface_loader: &khr::surface::Instance,
    surface: vk::SurfaceKHR,
    requested: vk::Extent2D,
    old: vk::SwapchainKHR,
) -> Result<(vk::SwapchainKHR, vk::Format, vk::Extent2D, Vec<vk::Image>), String> {
    let caps = unsafe { surface_loader.get_physical_device_surface_capabilities(vk.physical_device, surface) }
        .map_err(|e| format!("vkGetPhysicalDeviceSurfaceCapabilitiesKHR: {e:?}"))?;
    let formats = unsafe { surface_loader.get_physical_device_surface_formats(vk.physical_device, surface) }
        .map_err(|e| format!("vkGetPhysicalDeviceSurfaceFormatsKHR: {e:?}"))?;
    let format = formats
        .iter()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_UNORM && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .copied()
        .or_else(|| formats.first().copied())
        .ok_or("surface has no formats")?;

    let mut extent = requested;
    extent.width = extent.width.clamp(caps.min_image_extent.width, caps.max_image_extent.width);
    extent.height = extent.height.clamp(caps.min_image_extent.height, caps.max_image_extent.height);
    if caps.current_extent.width != u32::MAX {
        extent = caps.current_extent;
    }

    let mut image_count = caps.min_image_count.max(2);
    if caps.max_image_count > 0 {
        image_count = image_count.min(caps.max_image_count);
    }

    let alpha = if caps.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED) {
        vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED
    } else if caps.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED) {
        vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED
    } else if caps.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::INHERIT) {
        vk::CompositeAlphaFlagsKHR::INHERIT
    } else {
        vk::CompositeAlphaFlagsKHR::OPAQUE
    };

    let info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(caps.current_transform)
        .composite_alpha(alpha)
        .present_mode(vk::PresentModeKHR::FIFO)
        .clipped(true)
        .old_swapchain(old);

    let swapchain = unsafe { swapchain_loader.create_swapchain(&info, None) }
        .map_err(|e| format!("vkCreateSwapchainKHR failed: {e:?}"))?;
    let images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }
        .map_err(|e| format!("vkGetSwapchainImagesKHR failed: {e:?}"))?;
    println!(
        "[overlay] swapchain {}x{} format={:?} composite_alpha={:?} images={}",
        extent.width, extent.height, format.format, alpha, images.len()
    );
    Ok((swapchain, format.format, extent, images))
}

fn create_image_views(
    device: &ash::Device,
    format: vk::Format,
    images: &[vk::Image],
) -> Result<Vec<vk::ImageView>, String> {
    images
        .iter()
        .map(|&image| {
            let info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            unsafe { device.create_image_view(&info, None) }
                .map_err(|e| format!("vkCreateImageView (overlay) failed: {e:?}"))
        })
        .collect()
}

fn create_render_pass(device: &ash::Device, format: vk::Format) -> Result<vk::RenderPass, String> {
    let attachment = vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
    let color_ref = vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    };
    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(std::slice::from_ref(&color_ref));
    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
    let info = vk::RenderPassCreateInfo::default()
        .attachments(std::slice::from_ref(&attachment))
        .subpasses(std::slice::from_ref(&subpass))
        .dependencies(std::slice::from_ref(&dependency));
    unsafe { device.create_render_pass(&info, None) }
        .map_err(|e| format!("vkCreateRenderPass (overlay) failed: {e:?}"))
}

fn create_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    views: &[vk::ImageView],
) -> Result<Vec<vk::Framebuffer>, String> {
    views
        .iter()
        .map(|&view| {
            let info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(std::slice::from_ref(&view))
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            unsafe { device.create_framebuffer(&info, None) }
                .map_err(|e| format!("vkCreateFramebuffer failed: {e:?}"))
        })
        .collect()
}

fn create_quad_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
) -> Result<(vk::PipelineLayout, vk::Pipeline), String> {
    let vert = create_shader_module(device, OVERLAY_VERT_SPV)?;
    let frag = create_shader_module(device, OVERLAY_FRAG_SPV)?;
    let vert_stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::VERTEX)
        .module(vert)
        .name(c"main");
    let frag_stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::FRAGMENT)
        .module(frag)
        .name(c"main");
    let stages = [vert_stage, frag_stage];

    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let msaa = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let blend_attach = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let blend = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&blend_attach));
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let push = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(16);
    let layout_info = vk::PipelineLayoutCreateInfo::default().push_constant_ranges(std::slice::from_ref(&push));
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .map_err(|e| format!("vkCreatePipelineLayout (overlay) failed: {e:?}"))?;

    let create = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&msaa)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);
    let pipelines = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[create], None) }
        .map_err(|(_, e)| format!("vkCreateGraphicsPipelines (overlay) failed: {e:?}"))?;

    unsafe {
        device.destroy_shader_module(vert, None);
        device.destroy_shader_module(frag, None);
    }
    Ok((pipeline_layout, pipelines[0]))
}

fn create_shader_module(device: &ash::Device, bytes: &[u8]) -> Result<vk::ShaderModule, String> {
    let code = ash::util::read_spv(&mut std::io::Cursor::new(bytes))
        .map_err(|e| format!("overlay SPIR-V malformed: {e}"))?;
    let info = vk::ShaderModuleCreateInfo::default().code(&code);
    unsafe { device.create_shader_module(&info, None) }
        .map_err(|e| format!("vkCreateShaderModule (overlay) failed: {e:?}"))
}

/// Combined Wayland + Unix-socket loop. One thread: pipeline is `!Send`.
pub fn run(
    overlay: &mut Overlay,
    gpu: &mut OverlayGpu,
    vk: &VulkanContext,
    pipeline: &SplatPipeline,
    socket_path: &Path,
) -> Result<(), String> {
    let mut server = SocketServer::bind(socket_path).map_err(|e| format!("socket bind: {e}"))?;
    println!("[socket] listening on {}", socket_path.display());

    gpu.draw(vk)?;
    println!("[overlay] drew {QUAD_PX}x{QUAD_PX} pass-through quad");

    let wl_fd = overlay.event_queue.as_fd().as_raw_fd();
    loop {
        if overlay.closed() {
            println!("[overlay] compositor closed the layer surface");
            break;
        }
        overlay.flush_and_dispatch()?;
        if overlay.take_dirty() {
            let extent = overlay.extent();
            if extent.width != gpu.extent.width || extent.height != gpu.extent.height {
                gpu.recreate(vk, extent)?;
            }
            gpu.draw(vk)?;
        }

        server.pump(vk, pipeline).map_err(|e| format!("socket pump: {e}"))?;

        let mut fds = vec![
            libc::pollfd { fd: wl_fd, events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: server.listener_fd(), events: libc::POLLIN, revents: 0 },
        ];
        for fd in server.client_fds() {
            fds.push(libc::pollfd { fd, events: libc::POLLIN, revents: 0 });
        }

        overlay.event_queue.flush().map_err(|e| format!("wayland flush: {e}"))?;
        let read_guard = overlay.event_queue.prepare_read();
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 250) };
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                drop(read_guard);
                continue;
            }
            drop(read_guard);
            return Err(format!("poll: {err}"));
        }
        if fds[0].revents & libc::POLLIN != 0 {
            if let Some(guard) = read_guard {
                guard.read().map_err(|e| format!("wayland read: {e}"))?;
            }
        } else {
            drop(read_guard);
        }
    }
    Ok(())
}
