//! Present vs ingress are split:
//! - **Present:** a *movable* disc-sized layer-shell surface (compositor
//!   damage is the bubble, not the output). Empty input, exclusive_zone=-1.
//! - **Ingress:** when the Vulkan device supports zero-copy dmabuf import
//!   (`VK_EXT_image_drm_format_modifier` + `VK_KHR_external_memory_fd` +
//!   `VK_EXT_external_memory_dma_buf`), the compositor exports a DMA-BUF
//!   via `zwlr_export_dmabuf_manager_v1` and Vulkan imports it directly —
//!   no CPU-visible `wl_shm` buffer, no host readback, no pixel copies.
//!   When dmabuf is unavailable, falls back to the SHM screencopy path
//!   (a reused memfd of the full output at ≤10 fps, overlay parked
//!   off-screen for the copy).
//!   1080p RGBA is ~8MiB overwritten in place — the OOM was new shm +
//!   60fps fullscreen *present*, not "the GPU cannot sample 1080p".

use crate::actors::ingress::screen_dmabuf::DmabufScreenSource;
use crate::actors::ingress::screen_wlr::WlrScreenSource;
use crate::actors::ingress::{DmabufFrame, FrameSource, IngressFrame};
use crate::pipeline::SplatPipeline;
use crate::socket::SocketServer;
use crate::vulkan::{DmabufImportedImage, VulkanContext};
use ash::{khr, vk};
use std::io;
use std::os::fd::{AsFd, AsRawFd};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_region, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};
use wayland_client::protocol::wl_seat;
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1,
};

const OVERLAY_VERT_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/overlay_quad.vert.spv"));
const OVERLAY_FRAG_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/overlay_quad.frag.spv"));

/// Looking-glass diameter in pixels. Small enough not to dominate 1080p,
/// large enough to read the refraction.
const BUBBLE_PX: u32 = 220;
/// Capture/present ceiling. The OOM was a 16ms fullscreen copy+present spin.
const CAPTURE_INTERVAL: Duration = Duration::from_millis(100);
/// Hide before Omarchy's 150s screensaver so we are not painted over it.
const IDLE_HIDE_MS: u32 = 120_000;

pub struct Overlay {
    conn: Connection,
    event_queue: EventQueue<OverlayState>,
    state: OverlayState,
    /// One memfd reused for every full-output copy — never a new shm per frame.
    /// Only used by the SHM fallback path.
    cap_fd: Option<std::os::fd::OwnedFd>,
    cap_fd_size: usize,
    /// Top-left of the disc in output pixels. Quiescence / highlight actors
    /// will write this; `set_bubble_pos` is the whole present API.
    bubble_x: i32,
    bubble_y: i32,
}

pub struct OverlayState {
    compositor: Option<wl_compositor::WlCompositor>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    surface: Option<wl_surface::WlSurface>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    output: Option<wl_output::WlOutput>,
    shm: Option<wl_shm::WlShm>,
    screencopy: Option<zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1>,
    capture_spec: Option<(u32, u32, u32, wl_shm::Format)>,
    capture_dmabuf: bool,
    capture_done: bool,
    capture_ready: bool,
    capture_failed: bool,
    capture_y_invert: bool,
    output_width: u32,
    output_height: u32,
    seat: Option<wl_seat::WlSeat>,
    idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    idle: bool,
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
            output: None,
            shm: None,
            screencopy: None,
            capture_spec: None,
            capture_dmabuf: false,
            capture_done: false,
            capture_ready: false,
            capture_failed: false,
            capture_y_invert: false,
            output_width: 0,
            output_height: 0,
            seat: None,
            idle_notifier: None,
            idle: false,
            width: 0,
            height: 0,
            configured: false,
            closed: false,
            dirty: false,
        };
        event_queue
            .roundtrip(&mut state)
            .map_err(|e| format!("wayland registry roundtrip: {e}"))?;
        // Output mode arrives as a follow-up event.
        event_queue
            .roundtrip(&mut state)
            .map_err(|e| format!("wayland output roundtrip: {e}"))?;
        if state.output_width == 0 || state.output_height == 0 {
            state.output_width = 1920;
            state.output_height = 1080;
        }

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

        if let (Some(notifier), Some(seat)) = (state.idle_notifier.clone(), state.seat.clone()) {
            let _notification = notifier.get_idle_notification(IDLE_HIDE_MS, &seat, &qh, ());
            // Kept alive by the server as long as we don't destroy it; Dispatch
            // on OverlayState receives idled/resumed.
            std::mem::forget(_notification);
        }

        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            state.output.as_ref(),
            zwlr_layer_shell_v1::Layer::Overlay,
            "pachakutech-presence".into(),
            &qh,
            (),
        );
        // Disc-sized, not fullscreen. Top|Left + margins to center.
        layer_surface.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_keyboard_interactivity(
            zwlr_layer_surface_v1::KeyboardInteractivity::None,
        );
        layer_surface.set_size(BUBBLE_PX, BUBBLE_PX);
        let (ml, mt) = center_margins(state.output_width, state.output_height);
        layer_surface.set_margin(mt, 0, 0, ml);
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
            "[overlay] layer-shell disc {BUBBLE_PX}x{BUBBLE_PX} on {}x{} exclusive_zone=-1 input=pass-through",
            state.output_width, state.output_height
        );

        let (bx, by) = disc_origin(state.output_width, state.output_height);
        Ok(Self {
            conn,
            event_queue,
            state,
            cap_fd: None,
            cap_fd_size: 0,
            bubble_x: bx,
            bubble_y: by,
        })
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

    pub fn capture_extent(&self) -> vk::Extent2D {
        vk::Extent2D {
            width: self.state.output_width.max(1),
            height: self.state.output_height.max(1),
        }
    }

    /// Move the disc. Values are output-pixel top-left; clamped so the
    /// bubble stays on-screen. This is what a quiescence/highlight actor
    /// will call — the layer is not glued to the center.
    pub fn set_bubble_pos(&mut self, x: i32, y: i32) {
        let max_x = self.state.output_width.saturating_sub(BUBBLE_PX) as i32;
        let max_y = self.state.output_height.saturating_sub(BUBBLE_PX) as i32;
        self.bubble_x = x.clamp(0, max_x.max(0));
        self.bubble_y = y.clamp(0, max_y.max(0));
        if !self.state.idle {
            self.unpark_at_bubble();
        }
    }

    pub fn closed(&self) -> bool {
        self.state.closed
    }

    pub fn take_dirty(&mut self) -> bool {
        let d = self.state.dirty;
        self.state.dirty = false;
        d
    }

    fn park_offscreen(&self) {
        if let Some(ls) = &self.state.layer_surface {
            ls.set_margin(-((BUBBLE_PX as i32) * 4), 0, 0, 0);
            if let Some(s) = &self.state.surface {
                s.commit();
            }
        }
    }

    fn unpark_at_bubble(&self) {
        if let Some(ls) = &self.state.layer_surface {
            ls.set_margin(self.bubble_y, 0, 0, self.bubble_x);
            if let Some(s) = &self.state.surface {
                s.commit();
            }
        }
    }

    fn should_hide(&self, locked: bool, sleeping: bool) -> bool {
        self.state.idle || locked || sleeping
    }

    /// Full-output copy into the reused memfd (SHM fallback path). Caller
    /// parks the overlay off-screen first so this frame does not include
    /// our pixels.
    fn capture_full_output_shm(&mut self) -> Result<Option<IngressFrame>, String> {
        let qh = self.event_queue.handle();
        let output = self.state.output.clone().ok_or("no wl_output")?;
        let manager = self.state.screencopy.clone().ok_or("no wlr-screencopy")?;
        let shm = self.state.shm.clone().ok_or("no wl_shm")?;

        self.state.capture_spec = None;
        self.state.capture_dmabuf = false;
        self.state.capture_done = false;
        self.state.capture_ready = false;
        self.state.capture_failed = false;
        self.state.capture_y_invert = false;

        let frame = manager.capture_output(0, &output, &qh, ());
        for _ in 0..32 {
            self.event_queue
                .roundtrip(&mut self.state)
                .map_err(|e| format!("screencopy buffer roundtrip: {e}"))?;
            if self.state.capture_failed {
                eprintln!("[overlay] capture failed before buffer_done (shm={:?} dmabuf={})",
                    self.state.capture_spec.is_some(), self.state.capture_dmabuf);
                return Ok(None);
            }
            if self.state.capture_done {
                break;
            }
        }
        let (width, height, stride, format) = match self.state.capture_spec {
            Some(s) => s,
            None => {
                eprintln!(
                    "[overlay] capture: no wl_shm buffer advertised (dmabuf={} done={})",
                    self.state.capture_dmabuf, self.state.capture_done
                );
                return Ok(None);
            }
        };

        let size = stride as usize * height as usize;
        let shm_fd = if self.cap_fd_size >= size {
            self.cap_fd.as_ref().unwrap()
        } else {
            let fd = crate::actors::ingress::screen_wlr::create_anonymous_shm(size)?;
            self.cap_fd = Some(fd);
            self.cap_fd_size = size;
            self.cap_fd.as_ref().unwrap()
        };
        let pool = shm.create_pool(shm_fd.as_fd(), size as i32, &qh, ());
        let buffer = pool.create_buffer(0, width as i32, height as i32, stride as i32, format, &qh, ());
        frame.copy(&buffer);

        for _ in 0..24 {
            self.event_queue
                .roundtrip(&mut self.state)
                .map_err(|e| format!("screencopy ready roundtrip: {e}"))?;
            if self.state.capture_ready || self.state.capture_failed {
                break;
            }
        }
        if !self.state.capture_ready {
            eprintln!(
                "[overlay] capture: copy did not complete (failed={} shm={}x{} stride={})",
                self.state.capture_failed, width, height, stride
            );
            return Ok(None);
        }

        let mmap = crate::actors::ingress::screen_wlr::map_shm_readonly(&shm_fd, size)?;
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let y_invert = self.state.capture_y_invert;
        for yrow in 0..height as usize {
            let src_y = if y_invert { height as usize - 1 - yrow } else { yrow };
            let row_start = src_y * stride as usize;
            for xcol in 0..width as usize {
                let src = row_start + xcol * 4;
                let dst = (yrow * width as usize + xcol) * 4;
                rgba[dst] = mmap[src + 2];
                rgba[dst + 1] = mmap[src + 1];
                rgba[dst + 2] = mmap[src];
                rgba[dst + 3] = match format {
                    wl_shm::Format::Argb8888 => mmap[src + 3],
                    _ => 255,
                };
            }
        }
        Ok(Some(IngressFrame { width, height, rgba }))
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
                "wl_output" if state.output.is_none() => {
                    state.output = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "wl_shm" if state.shm.is_none() => {
                    state.shm = Some(registry.bind(name, version.min(1), qh, ()));
                }
                "zwlr_layer_shell_v1" if state.layer_shell.is_none() => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "zwlr_screencopy_manager_v1" if state.screencopy.is_none() => {
                    state.screencopy = Some(registry.bind(name, version.min(3), qh, ()));
                }
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, version.min(7), qh, ()));
                }
                "ext_idle_notifier_v1" if state.idle_notifier.is_none() => {
                    state.idle_notifier = Some(registry.bind(name, version.min(1), qh, ()));
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

impl Dispatch<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, ()> for OverlayState {
    fn event(
        state: &mut Self,
        _frame: &zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer { format, width, height, stride } => {
                let format = match format {
                    WEnum::Value(f) => f,
                    WEnum::Unknown(_) => wl_shm::Format::Argb8888,
                };
                state.capture_spec = Some((width, height, stride, format));
            }
            zwlr_screencopy_frame_v1::Event::LinuxDmabuf { .. } => state.capture_dmabuf = true,
            zwlr_screencopy_frame_v1::Event::BufferDone => state.capture_done = true,
            zwlr_screencopy_frame_v1::Event::Ready { .. } => state.capture_ready = true,
            zwlr_screencopy_frame_v1::Event::Failed => state.capture_failed = true,
            zwlr_screencopy_frame_v1::Event::Flags { flags } => {
                if let WEnum::Value(f) = flags {
                    state.capture_y_invert =
                        f.contains(zwlr_screencopy_frame_v1::Flags::YInvert);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for OverlayState {
    fn event(
        state: &mut Self,
        _proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Mode { flags, width, height, .. } = event {
            let current = match flags {
                WEnum::Value(f) => f.contains(wl_output::Mode::Current),
                _ => true,
            };
            if current && width > 0 && height > 0 {
                state.output_width = width as u32;
                state.output_height = height as u32;
            }
        }
    }
}

impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, ()> for OverlayState {
    fn event(
        state: &mut Self,
        _proxy: &ext_idle_notification_v1::ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            ext_idle_notification_v1::Event::Idled => state.idle = true,
            ext_idle_notification_v1::Event::Resumed => state.idle = false,
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(OverlayState: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(OverlayState: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(OverlayState: ignore wl_region::WlRegion);
wayland_client::delegate_noop!(OverlayState: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(OverlayState: ignore ext_idle_notifier_v1::ExtIdleNotifierV1);
wayland_client::delegate_noop!(OverlayState: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(OverlayState: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(OverlayState: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(OverlayState: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
wayland_client::delegate_noop!(OverlayState: ignore zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1);

// ===========================================================================
// GPU: swapchain + looking-glass pipeline targeting the layer-shell surface
// ===========================================================================

/// Swapchain + looking-glass pipeline targeting the layer-shell surface.
/// The screen feed can be either SHM-based (CPU upload via staging buffer)
/// or dmabuf-based (zero-copy Vulkan import), selected at construction
/// based on whether the device supports `VK_EXT_image_drm_format_modifier`.
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
    screen: ScreenFeedKind,
    pending_update: bool,
}

/// Which screen-feeding strategy is active. Both variants expose the same
/// `descriptor_set` / `descriptor_set_layout` that `OverlayGpu` binds.
enum ScreenFeedKind {
    /// SHM fallback: host-visible staging buffer + sampled GPU image,
    /// re-uploaded from CPU RGBA each frame.
    Shm(ScreenFeed),
    /// Zero-copy: compositor-exported dmabuf imported directly into a
    /// sampled `VkImage` via `VkImportMemoryFdInfoKHR`. One imported
    /// image per captured frame, retired after the GPU fence signals.
    Dmabuf(DmabufScreenFeed),
}

impl ScreenFeedKind {
    fn descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        match self {
            Self::Shm(f) => f.descriptor_set_layout,
            Self::Dmabuf(f) => f.descriptor_set_layout,
        }
    }

    fn descriptor_set(&self) -> vk::DescriptorSet {
        match self {
            Self::Shm(f) => f.descriptor_set,
            Self::Dmabuf(f) => f.descriptor_set,
        }
    }

    fn destroy(&mut self, vk: &VulkanContext) {
        match self {
            Self::Shm(f) => f.destroy(&vk.device),
            Self::Dmabuf(f) => f.destroy(vk),
        }
    }
}

/// Host-visible staging buffer + sampled GPU image of the last screen capture.
/// Used by the SHM fallback path.
struct ScreenFeed {
    image: vk::Image,
    image_mem: vk::DeviceMemory,
    view: vk::ImageView,
    sampler: vk::Sampler,
    staging: vk::Buffer,
    staging_mem: vk::DeviceMemory,
    staging_ptr: *mut u8,
    staging_size: vk::DeviceSize,
    width: u32,
    height: u32,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    image_layout: vk::ImageLayout,
}

/// Zero-copy dmabuf screen feed. Manages a rotating pair of imported
/// `DmabufImportedImage`s: the *current* one (being sampled this frame)
/// and the *previous* one (retired after the GPU fence signals).
///
/// Lifecycle per captured frame:
/// 1. `import_frame(vk, frame)` — moves current→previous, imports the
///    new dmabuf fd into a fresh `VkImage`, updates the descriptor set.
/// 2. `record_transition(device, cmd)` — inserts an image layout barrier
///    (`UNDEFINED`→`SHADER_READ_ONLY_OPTIMAL`) with
///    `VK_QUEUE_FAMILY_EXTERNAL` as the source queue family.
/// 3. After the render fence signals (next `draw`), `retire_previous`
///    destroys the old imported image.
struct DmabufScreenFeed {
    sampler: vk::Sampler,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    current: Option<DmabufImportedImage>,
    previous: Option<DmabufImportedImage>,
    needs_transition: bool,
}

impl OverlayGpu {
    pub fn new(vk: &VulkanContext, extent: vk::Extent2D, capture: vk::Extent2D) -> Result<Self, String> {
        let surface = vk.surface.ok_or("VulkanContext has no Wayland VkSurface")?;
        let surface_loader = vk.surface_loader.as_ref().ok_or("no KHR_surface loader")?;
        let swapchain_loader = khr::swapchain::Device::new(&vk.instance, &vk.device);

        let (swapchain, format, extent, images) =
            create_swapchain(vk, &swapchain_loader, surface_loader, surface, extent, vk::SwapchainKHR::null())?;

        let image_views = create_image_views(&vk.device, format, &images)?;
        let render_pass = create_render_pass(&vk.device, format)?;
        let framebuffers = create_framebuffers(&vk.device, render_pass, extent, &image_views)?;

        // Choose the screen feed strategy based on device capability.
        let screen = if vk.supports_zero_copy_capture() {
            println!("[overlay] using zero-copy dmabuf screen feed (VK_EXT_image_drm_format_modifier enabled)");
            ScreenFeedKind::Dmabuf(DmabufScreenFeed::new(vk)?)
        } else {
            println!("[overlay] using SHM screen feed (dmabuf import not supported on this device)");
            ScreenFeedKind::Shm(ScreenFeed::new(vk, capture.width.max(1), capture.height.max(1))?)
        };

        let (pipeline_layout, pipeline) =
            create_quad_pipeline(&vk.device, render_pass, screen.descriptor_set_layout())?;

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
            screen,
            pending_update: true,
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
        let (layout, pipeline) =
            create_quad_pipeline(&vk.device, self.render_pass, self.screen.descriptor_set_layout())?;
        unsafe {
            vk.device.destroy_pipeline(self.pipeline, None);
            vk.device.destroy_pipeline_layout(self.pipeline_layout, None);
        }
        self.pipeline_layout = layout;
        self.pipeline = pipeline;
        Ok(())
    }

    /// SHM path: CPU-side RGBA8 (top-left) → staging buffer. Copied to
    /// the sampled image at the start of the next `draw`.
    pub fn upload_screen(&mut self, frame: &IngressFrame) {
        if let ScreenFeedKind::Shm(ref mut feed) = self.screen {
            feed.write_rgba(frame);
            self.pending_update = true;
        }
    }

    /// Dmabuf path: zero-copy import of a compositor-exported DMA-BUF into
    /// a sampled `VkImage`. Retires the previous imported image (after the
    /// last frame's fence has signalled). The descriptor set is updated to
    /// point to the new image view.
    pub fn import_screen(&mut self, vk: &VulkanContext, frame: &DmabufFrame) {
        if let ScreenFeedKind::Dmabuf(ref mut feed) = self.screen {
            // Retire the previous frame's imported image — the fence from
            // the last draw has already been waited on by `draw`.
            feed.retire_previous(vk);
            match feed.import_frame(vk, frame) {
                Ok(()) => self.pending_update = true,
                Err(e) => {
                    eprintln!("[overlay] dmabuf import failed, retaining last frame: {e}");
                }
            }
        }
    }

    pub fn draw(&mut self, vk: &VulkanContext) -> Result<(), String> {
        unsafe { vk.device.wait_for_fences(&[self.in_flight], true, u64::MAX) }
            .map_err(|e| format!("overlay fence wait: {e:?}"))?;

        // The previous draw has completed. For the dmabuf path, retire
        // the old imported image now (it's safe to destroy).
        if let ScreenFeedKind::Dmabuf(ref mut feed) = self.screen {
            feed.retire_previous(vk);
        }

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

        if self.pending_update {
            match &mut self.screen {
                ScreenFeedKind::Shm(feed) => feed.record_upload(&vk.device, self.command_buffer),
                ScreenFeedKind::Dmabuf(feed) => {
                    feed.record_transition(&vk.device, self.command_buffer);
                }
            }
            self.pending_update = false;
        }

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
            vk.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.screen.descriptor_set()],
                &[],
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
            let pc = [self.extent.width as f32, self.extent.height as f32, 0.0, 0.0];
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
        let wait_stages = [vk::PipelineStageFlags::TRANSFER | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
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
            self.screen.destroy(vk);
            vk.device.destroy_command_pool(self.command_pool, None);
            vk.device.destroy_semaphore(self.image_available, None);
            vk.device.destroy_semaphore(self.render_finished, None);
            vk.device.destroy_fence(self.in_flight, None);
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);
        }
    }
}

// ---------------------------------------------------------------------------
// DmabufScreenFeed implementation
// ---------------------------------------------------------------------------

impl DmabufScreenFeed {
    fn new(vk: &VulkanContext) -> Result<Self, String> {
        let device = &vk.device;

        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
        let sampler = unsafe { device.create_sampler(&sampler_info, None) }
            .map_err(|e| format!("vkCreateSampler (dmabuf) failed: {e:?}"))?;

        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let dsl_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(std::slice::from_ref(&binding));
        let descriptor_set_layout = unsafe { device.create_descriptor_set_layout(&dsl_info, None) }
            .map_err(|e| format!("vkCreateDescriptorSetLayout (dmabuf) failed: {e:?}"))?;

        let pool_size = vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1);
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(std::slice::from_ref(&pool_size));
        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_info, None) }
            .map_err(|e| format!("vkCreateDescriptorPool (dmabuf) failed: {e:?}"))?;

        let alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&descriptor_set_layout));
        let descriptor_set = unsafe { device.allocate_descriptor_sets(&alloc) }
            .map_err(|e| format!("vkAllocateDescriptorSets (dmabuf) failed: {e:?}"))?[0];

        Ok(Self {
            sampler,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_set,
            current: None,
            previous: None,
            needs_transition: false,
        })
    }

    /// Imports a compositor-exported dmabuf into a fresh `VkImage`, updates
    /// the descriptor set to point at the new image view, and marks that a
    /// layout transition is needed before the next draw.
    ///
    /// The previous current image (if any) is moved to `previous` for
    /// retirement after the next GPU fence.
    fn import_frame(&mut self, vk: &VulkanContext, frame: &DmabufFrame) -> Result<(), String> {
        let imported = vk.import_dmabuf(frame)?;

        // Move current → previous (will be retired after the next fence).
        self.previous = self.current.take();
        self.current = Some(imported);

        // Update the descriptor set to sample the new image view.
        let image_info = vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(self.current.as_ref().unwrap().view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));
        unsafe { vk.device.update_descriptor_sets(&[write], &[]) };

        self.needs_transition = true;
        Ok(())
    }

    /// Records the image layout transition barrier for the current imported
    /// image. Called once before the render pass, in the same command
    /// buffer as the draw.
    fn record_transition(&mut self, device: &ash::Device, cmd: vk::CommandBuffer) {
        if !self.needs_transition {
            return;
        }
        if let Some(ref imported) = self.current {
            VulkanContext::record_dmabuf_transition(device, cmd, imported.image);
        }
        self.needs_transition = false;
    }

    /// Destroys the previous imported image. Safe to call after the GPU
    /// fence from the last draw has signalled (i.e. at the start of
    /// `draw` or `import_frame`).
    fn retire_previous(&mut self, vk: &VulkanContext) {
        if let Some(mut prev) = self.previous.take() {
            vk.destroy_dmabuf_import(&mut prev);
        }
    }

    fn destroy(&mut self, vk: &VulkanContext) {
        // Destroy both current and previous imported images.
        if let Some(mut img) = self.current.take() {
            vk.destroy_dmabuf_import(&mut img);
        }
        if let Some(mut img) = self.previous.take() {
            vk.destroy_dmabuf_import(&mut img);
        }
        unsafe {
            vk.device.destroy_descriptor_pool(self.descriptor_pool, None);
            vk.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            vk.device.destroy_sampler(self.sampler, None);
        }
    }
}

// ---------------------------------------------------------------------------
// ScreenFeed (SHM fallback) implementation — unchanged from the original
// ---------------------------------------------------------------------------

impl ScreenFeed {
    fn new(vk: &VulkanContext, width: u32, height: u32) -> Result<Self, String> {
        let device = &vk.device;
        let width = width.max(1);
        let height = height.max(1);
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { device.create_image(&image_info, None) }
            .map_err(|e| format!("vkCreateImage (screen) failed: {e:?}"))?;
        let img_req = unsafe { device.get_image_memory_requirements(image) };
        let img_mem = alloc_memory(vk, img_req, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        unsafe { device.bind_image_memory(image, img_mem, 0) }
            .map_err(|e| format!("vkBindImageMemory (screen) failed: {e:?}"))?;

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let view = unsafe { device.create_image_view(&view_info, None) }
            .map_err(|e| format!("vkCreateImageView (screen) failed: {e:?}"))?;

        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
        let sampler = unsafe { device.create_sampler(&sampler_info, None) }
            .map_err(|e| format!("vkCreateSampler failed: {e:?}"))?;

        let staging_size = (width as vk::DeviceSize) * (height as vk::DeviceSize) * 4;
        let buf_info = vk::BufferCreateInfo::default()
            .size(staging_size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let staging = unsafe { device.create_buffer(&buf_info, None) }
            .map_err(|e| format!("vkCreateBuffer (screen staging) failed: {e:?}"))?;
        let buf_req = unsafe { device.get_buffer_memory_requirements(staging) };
        let staging_mem = alloc_memory(
            vk,
            buf_req,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        unsafe { device.bind_buffer_memory(staging, staging_mem, 0) }
            .map_err(|e| format!("vkBindBufferMemory (screen staging) failed: {e:?}"))?;
        let staging_ptr = unsafe {
            device.map_memory(staging_mem, 0, staging_size, vk::MemoryMapFlags::empty())
        }
        .map_err(|e| format!("vkMapMemory (screen staging) failed: {e:?}"))? as *mut u8;

        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let dsl_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(std::slice::from_ref(&binding));
        let descriptor_set_layout = unsafe { device.create_descriptor_set_layout(&dsl_info, None) }
            .map_err(|e| format!("vkCreateDescriptorSetLayout (screen) failed: {e:?}"))?;
        let pool_size = vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1);
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(std::slice::from_ref(&pool_size));
        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_info, None) }
            .map_err(|e| format!("vkCreateDescriptorPool (screen) failed: {e:?}"))?;
        let alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&descriptor_set_layout));
        let descriptor_set = unsafe { device.allocate_descriptor_sets(&alloc) }
            .map_err(|e| format!("vkAllocateDescriptorSets (screen) failed: {e:?}"))?[0];

        let image_info = vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));
        unsafe { device.update_descriptor_sets(&[write], &[]) };

        // Seed staging with opaque dark so the first draw isn't garbage.
        unsafe { std::ptr::write_bytes(staging_ptr, 20, staging_size as usize) };

        Ok(Self {
            image,
            image_mem: img_mem,
            view,
            sampler,
            staging,
            staging_mem,
            staging_ptr,
            staging_size,
            width,
            height,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_set,
            image_layout: vk::ImageLayout::UNDEFINED,
        })
    }

    fn write_rgba(&mut self, frame: &IngressFrame) {
        let w = self.width.min(frame.width) as usize;
        let h = self.height.min(frame.height) as usize;
        let src_stride = frame.width as usize * 4;
        let dst_stride = self.width as usize * 4;
        unsafe {
            for y in 0..h {
                let src = frame.rgba.as_ptr().add(y * src_stride);
                let dst = self.staging_ptr.add(y * dst_stride);
                std::ptr::copy_nonoverlapping(src, dst, w * 4);
            }
        }
    }

    fn record_upload(&mut self, device: &ash::Device, cmd: vk::CommandBuffer) {
        let sub = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        let to_dst = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(self.image_layout)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .image(self.image)
            .subresource_range(sub);
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_dst],
            );
        }
        let region = vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_extent(vk::Extent3D { width: self.width, height: self.height, depth: 1 });
        unsafe {
            device.cmd_copy_buffer_to_image(
                cmd,
                self.staging,
                self.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );
        }
        let to_sample = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image(self.image)
            .subresource_range(sub);
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_sample],
            );
        }
        self.image_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
    }

    fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            device.destroy_sampler(self.sampler, None);
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.image_mem, None);
            device.unmap_memory(self.staging_mem);
            device.destroy_buffer(self.staging, None);
            device.free_memory(self.staging_mem, None);
        }
    }
}

fn alloc_memory(
    vk: &VulkanContext,
    req: vk::MemoryRequirements,
    flags: vk::MemoryPropertyFlags,
) -> Result<vk::DeviceMemory, String> {
    let props = unsafe { vk.instance.get_physical_device_memory_properties(vk.physical_device) };
    let mut type_index = None;
    for i in 0..props.memory_type_count {
        if req.memory_type_bits & (1 << i) != 0
            && props.memory_types[i as usize].property_flags.contains(flags)
        {
            type_index = Some(i);
            break;
        }
    }
    let memory_type_index = type_index.ok_or("no matching memory type for screen feed")?;
    let alloc = vk::MemoryAllocateInfo::default()
        .allocation_size(req.size)
        .memory_type_index(memory_type_index);
    unsafe { vk.device.allocate_memory(&alloc, None) }
        .map_err(|e| format!("vkAllocateMemory (screen) failed: {e:?}"))
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
    descriptor_set_layout: vk::DescriptorSetLayout,
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
    let set_layouts = [descriptor_set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(std::slice::from_ref(&push));
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

fn center_margins(output_w: u32, output_h: u32) -> (i32, i32) {
    let w = output_w.max(BUBBLE_PX);
    let h = output_h.max(BUBBLE_PX);
    (((w - BUBBLE_PX) / 2) as i32, ((h - BUBBLE_PX) / 2) as i32)
}

fn disc_origin(output_w: u32, output_h: u32) -> (i32, i32) {
    center_margins(output_w, output_h)
}

fn spawn_hypr_lock_watch(locked: Arc<AtomicBool>, screensaver: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("presence-hypr-lock".into())
        .spawn(move || {
            let Ok(sig) = std::env::var("HYPRLAND_INSTANCE_SIGNATURE") else { return };
            let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
            let path = Path::new(&runtime).join("hypr").join(sig).join(".socket2.sock");
            let Ok(stream) = std::os::unix::net::UnixStream::connect(path) else { return };
            let reader = io::BufReader::new(stream);
            for line in io::BufRead::lines(reader) {
                let Ok(line) = line else { break };
                if line.starts_with("lock") {
                    locked.store(true, Ordering::Relaxed);
                } else if line.starts_with("unlock") {
                    locked.store(false, Ordering::Relaxed);
                } else if line.contains("org.omarchy.screensaver") {
                    if line.starts_with("openwindow") {
                        screensaver.store(true, Ordering::Relaxed);
                    } else if line.starts_with("closewindow") {
                        screensaver.store(false, Ordering::Relaxed);
                    }
                }
            }
        })
        .ok();
}

fn spawn_sleep_watch(sleeping: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("presence-sleep".into())
        .spawn(move || {
            let child = std::process::Command::new("dbus-monitor")
                .args([
                    "--system",
                    "type='signal',interface='org.freedesktop.login1.Manager',member='PrepareForSleep'",
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn();
            let Ok(mut child) = child else { return };
            let Some(stdout) = child.stdout.take() else { return };
            let reader = io::BufReader::new(stdout);
            for line in io::BufRead::lines(reader) {
                let Ok(line) = line else { break };
                let l = line.to_ascii_lowercase();
                if l.contains("boolean true") {
                    sleeping.store(true, Ordering::Relaxed);
                } else if l.contains("boolean false") {
                    sleeping.store(false, Ordering::Relaxed);
                }
            }
        })
        .ok();
}

/// Message type for the capture thread → main loop channel. Either a
/// zero-copy dmabuf frame or a CPU-side SHM frame.
enum CaptureMsg {
    Dmabuf(DmabufFrame),
    Shm(IngressFrame),
}

/// Combined Wayland + Unix-socket loop. One thread: pipeline is `!Send`.
///
/// When the Vulkan device supports zero-copy dmabuf import, spawns a
/// `DmabufScreenSource` thread that captures via
/// `zwlr_export_dmabuf_manager_v1` and sends `DmabufFrame`s. Each frame
/// is imported directly into a sampled `VkImage` — no CPU pixel copy.
///
/// Falls back to the SHM screencopy path (`WlrScreenSource`) when dmabuf
/// is unavailable, retaining the last-good GPU texture and rate-limiting
/// retries if the dmabuf path fails at runtime.
pub fn run(
    overlay: &mut Overlay,
    gpu: &mut OverlayGpu,
    vk: &VulkanContext,
    pipeline: &SplatPipeline,
    socket_path: &Path,
) -> Result<(), String> {
    let mut server = SocketServer::bind(socket_path).map_err(|e| format!("socket bind: {e}"))?;
    println!("[socket] listening on {}", socket_path.display());

    let locked = Arc::new(AtomicBool::new(false));
    let screensaver = Arc::new(AtomicBool::new(false));
    let sleeping = Arc::new(AtomicBool::new(false));
    spawn_hypr_lock_watch(locked.clone(), screensaver.clone());
    spawn_sleep_watch(sleeping.clone());

    let mut last_capture = Instant::now() - CAPTURE_INTERVAL;
    let mut parked = false;
    let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel::<CaptureMsg>(1);

    // Choose capture strategy based on device capability.
    let use_dmabuf = vk.supports_zero_copy_capture();

    if use_dmabuf {
        // --- Zero-copy dmabuf capture thread ---
        std::thread::Builder::new()
            .name("presence-dmabuf-capture".into())
            .spawn(move || {
                let mut src = match DmabufScreenSource::connect() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("[overlay] dmabuf capture client: {e}");
                        eprintln!("[overlay] dmabuf unavailable — falling back to SHM screencopy");
                        // Fall back to SHM in the same thread.
                        let mut shm_src = match WlrScreenSource::connect() {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("[overlay] SHM fallback also failed: {e}");
                                return;
                            }
                        };
                        loop {
                            match shm_src.next_frame() {
                                Ok(Some(frame)) => {
                                    let _ = frame_tx.send(CaptureMsg::Shm(frame));
                                }
                                Ok(None) => {}
                                Err(e) => eprintln!("[overlay] SHM capture: {e}"),
                            }
                            std::thread::sleep(CAPTURE_INTERVAL);
                        }
                    }
                };
                let mut consecutive_failures = 0u32;
                loop {
                    match src.next_frame() {
                        Ok(Some(frame)) => {
                            consecutive_failures = 0;
                            let _ = frame_tx.send(CaptureMsg::Dmabuf(frame));
                        }
                        Ok(None) => {
                            // Cancelled (temporary) — retry after backoff.
                            consecutive_failures = consecutive_failures.saturating_add(1);
                        }
                        Err(e) => {
                            consecutive_failures = consecutive_failures.saturating_add(1);
                            eprintln!("[overlay] dmabuf capture error: {e}");
                        }
                    }
                    // Rate-limit after repeated failures to avoid log spam.
                    let delay = if consecutive_failures > 3 {
                        CAPTURE_INTERVAL * 3
                    } else {
                        CAPTURE_INTERVAL
                    };
                    std::thread::sleep(delay);
                }
            })
            .map_err(|e| format!("dmabuf capture thread: {e}"))?;
        println!("[overlay] zero-copy dmabuf capture enabled (wlr-export-dmabuf → Vulkan import)");
    } else {
        // --- SHM screencopy fallback thread ---
        std::thread::Builder::new()
            .name("presence-screencopy".into())
            .spawn(move || {
                let mut src = match WlrScreenSource::connect() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("[overlay] screencopy client: {e}");
                        return;
                    }
                };
                loop {
                    match src.next_frame() {
                        Ok(Some(frame)) => {
                            let _ = frame_tx.send(CaptureMsg::Shm(frame));
                        }
                        Ok(None) => {}
                        Err(e) => eprintln!("[overlay] screencopy: {e}"),
                    }
                    std::thread::sleep(CAPTURE_INTERVAL);
                }
            })
            .map_err(|e| format!("screencopy thread: {e}"))?;
        println!(
            "[overlay] disc {BUBBLE_PX}px movable, full-output ingress ≤10fps on a second Wayland client (SHM)"
        );
    }

    gpu.draw(vk)?;

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
        }

        server.pump(vk, pipeline).map_err(|e| format!("socket pump: {e}"))?;

        let hide = overlay.should_hide(
            locked.load(Ordering::Relaxed) || screensaver.load(Ordering::Relaxed),
            sleeping.load(Ordering::Relaxed),
        );
        if hide {
            if !parked {
                overlay.park_offscreen();
                parked = true;
                println!("[overlay] hidden (idle/lock/sleep)");
            }
        } else {
            if parked {
                overlay.unpark_at_bubble();
                parked = false;
            }
            let mut got = false;
            while let Ok(msg) = frame_rx.try_recv() {
                static ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                if !ONCE.swap(true, Ordering::Relaxed) {
                    match &msg {
                        CaptureMsg::Dmabuf(f) => println!(
                            "[overlay] dmabuf capture ok {}x{} format=0x{:08X}",
                            f.width, f.height, f.drm_format
                        ),
                        CaptureMsg::Shm(f) => println!(
                            "[overlay] capture ok {}x{} ({} bytes)",
                            f.width, f.height, f.rgba.len()
                        ),
                    }
                }
                match msg {
                    CaptureMsg::Dmabuf(frame) => {
                        gpu.import_screen(vk, &frame);
                    }
                    CaptureMsg::Shm(frame) => {
                        gpu.upload_screen(&frame);
                    }
                }
                got = true;
            }
            if got || last_capture.elapsed() >= CAPTURE_INTERVAL {
                gpu.draw(vk)?;
                last_capture = Instant::now();
            }
        }

        let mut fds = vec![
            libc::pollfd { fd: wl_fd, events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: server.listener_fd(), events: libc::POLLIN, revents: 0 },
        ];
        for fd in server.client_fds() {
            fds.push(libc::pollfd { fd, events: libc::POLLIN, revents: 0 });
        }

        overlay.event_queue.flush().map_err(|e| format!("wayland flush: {e}"))?;
        let read_guard = overlay.event_queue.prepare_read();
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 100) };
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
