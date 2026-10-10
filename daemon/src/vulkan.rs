//! Owns the GPU. Deliberately minimal for now: create an Instance, pick a
//! physical device, report whether it can do the zero-copy dma_buf import
//! path the architecture depends on, and create a logical Device + queue.
//! No swapchain, no rendering yet — that's the next layer up, and it's
//! yours to build (see README.md "What's next").

use ash::{vk, Entry};
use std::ffi::CStr;
use std::os::fd::AsRawFd;
use std::ffi::c_void;

use crate::actors::ingress::{DmabufFrame, DRM_FORMAT_MOD_INVALID};

pub struct VulkanContext {
    pub entry: Entry,
    pub instance: ash::Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: ash::Device,
    pub graphics_queue_family: u32,
    /// The one queue this daemon uses for everything, compute included —
    /// a queue from a `GRAPHICS`-capable family always supports `COMPUTE`
    /// too (the spec guarantees `GRAPHICS` implies `COMPUTE`), so there's
    /// no need for a second, separate compute queue family lookup yet. A
    /// dedicated async-compute queue would be the thing to add if the
    /// projection/eviction dispatches ever need to run concurrently with
    /// real rendering work instead of just before it.
    pub graphics_queue: vk::Queue,
    pub supports_dma_buf_import: bool,
    /// `true` when `VK_EXT_image_drm_format_modifier` is available and
    /// enabled on the device. Required for importing modifier-tiled dmabufs
    /// (which Hyprland commonly exports) via the explicit plane-layout
    /// create-info chain.
    pub supports_drm_format_modifier: bool,
    /// Device-level function loader for `VK_KHR_external_memory_fd`.
    /// Provides `get_memory_fd_properties` — the call that tells us which
    /// Vulkan memory types are compatible with a given dmabuf fd.
    pub external_memory_fd: Option<ash::khr::external_memory_fd::Device>,
    /// Queue family a dmabuf acquire barrier uses as `srcQueueFamilyIndex`.
    /// `VK_QUEUE_FAMILY_FOREIGN_EXT` when that extension is enabled (the
    /// compositor is outside this Vulkan instance); otherwise
    /// `VK_QUEUE_FAMILY_EXTERNAL`.
    pub dmabuf_src_queue_family: u32,
    pub device_name: String,
    /// Presentable Wayland `VkSurfaceKHR` when the overlay is up. Destroyed
    /// in `Drop` before the instance.
    pub surface: Option<vk::SurfaceKHR>,
    pub surface_loader: Option<ash::khr::surface::Instance>,
}

impl VulkanContext {
    pub fn init() -> Result<Self, String> {
        Self::init_inner(None)
    }

    pub fn init_for_wayland(
        display: *mut vk::wl_display,
        surface: *mut vk::wl_surface,
    ) -> Result<Self, String> {
        Self::init_inner(Some((display, surface)))
    }

    fn init_inner(wayland: Option<(*mut vk::wl_display, *mut vk::wl_surface)>) -> Result<Self, String> {
        // SAFETY: `Entry::load` dynamically loads the system Vulkan loader.
        // This is the one place in the whole daemon where "no GPU/driver
        // present" is expected and handled, rather than a bug.
        let entry = unsafe { Entry::load() }
            .map_err(|e| format!("no Vulkan loader found on this system: {e}"))?;

        let app_info = vk::ApplicationInfo::default()
            .application_name(c"pachakutech-presence-daemon")
            .api_version(vk::API_VERSION_1_1); // AHardwareBuffer/dma_buf import needs >= 1.1

        let mut instance_exts: Vec<*const i8> = Vec::new();
        if wayland.is_some() {
            instance_exts.push(ash::khr::surface::NAME.as_ptr());
            instance_exts.push(ash::khr::wayland_surface::NAME.as_ptr());
        }
        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&instance_exts);
        let instance = unsafe { entry.create_instance(&create_info, None) }
            .map_err(|e| format!("vkCreateInstance failed: {e:?}"))?;

        let (surface, surface_loader) = if let Some((display, wl_surface)) = wayland {
            let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
            let wayland_loader = ash::khr::wayland_surface::Instance::new(&entry, &instance);
            let info = vk::WaylandSurfaceCreateInfoKHR::default()
                .display(display)
                .surface(wl_surface);
            let surface = unsafe { wayland_loader.create_wayland_surface(&info, None) }
                .map_err(|e| format!("vkCreateWaylandSurfaceKHR failed: {e:?}"))?;
            (Some(surface), Some(surface_loader))
        } else {
            (None, None)
        };

        let physical_devices = unsafe { instance.enumerate_physical_devices() }
            .map_err(|e| format!("failed to enumerate physical devices: {e:?}"))?;
        if physical_devices.is_empty() {
            unsafe {
                if let (Some(s), Some(l)) = (surface, surface_loader.as_ref()) {
                    l.destroy_surface(s, None);
                }
                instance.destroy_instance(None);
            };
            return Err("no Vulkan-capable physical devices found".into());
        }

        let (physical_device, graphics_queue_family, device_name) = pick_physical_device(
            &instance,
            &physical_devices,
            surface_loader.as_ref(),
            surface,
        )?;

        let available_extensions =
            unsafe { instance.enumerate_device_extension_properties(physical_device) }
                .map_err(|e| format!("failed to enumerate device extensions: {e:?}"))?;
        let has_extension = |name: &CStr| {
            available_extensions.iter().any(|ext| {
                (unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) }) == name
            })
        };
        let supports_dma_buf_import = has_extension(ash::khr::external_memory_fd::NAME)
            && has_extension(ash::ext::external_memory_dma_buf::NAME);
        let supports_drm_format_modifier =
            has_extension(ash::ext::image_drm_format_modifier::NAME);
        let supports_foreign_queue = has_extension(ash::ext::queue_family_foreign::NAME);

        let queue_priorities = [1.0f32];
        let queue_create_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(graphics_queue_family)
            .queue_priorities(&queue_priorities);
        let queue_create_infos = [queue_create_info];

        // Request the zero-copy extensions when present; fall back cleanly
        // when they're not. Swapchain is required when presenting to the overlay.
        let mut enabled_extensions: Vec<*const i8> = Vec::new();
        if wayland.is_some() {
            enabled_extensions.push(ash::khr::swapchain::NAME.as_ptr());
        }
        if supports_dma_buf_import {
            enabled_extensions.push(ash::khr::external_memory_fd::NAME.as_ptr());
            enabled_extensions.push(ash::ext::external_memory_dma_buf::NAME.as_ptr());
        }
        if supports_drm_format_modifier {
            enabled_extensions.push(ash::ext::image_drm_format_modifier::NAME.as_ptr());
        }
        if supports_dma_buf_import && supports_foreign_queue {
            enabled_extensions.push(ash::ext::queue_family_foreign::NAME.as_ptr());
        }
        let dmabuf_src_queue_family = if supports_dma_buf_import && supports_foreign_queue {
            vk::QUEUE_FAMILY_FOREIGN_EXT
        } else {
            vk::QUEUE_FAMILY_EXTERNAL
        };

        // R8G8B8A8 storage images (the screen-tile atlas) require this
        // feature. Every other feature stays off, matching the previous
        // device (pEnabledFeatures was null).
        let supported = unsafe { instance.get_physical_device_features(physical_device) };
        let mut enabled_features = vk::PhysicalDeviceFeatures::default();
        enabled_features.shader_storage_image_extended_formats = supported.shader_storage_image_extended_formats;
        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&enabled_extensions)
            .enabled_features(&enabled_features);

        let device = unsafe { instance.create_device(physical_device, &device_create_info, None) }
            .map_err(|e| format!("vkCreateDevice failed: {e:?}"))?;
        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family, 0) };

        // Load the device-level function loader for VK_KHR_external_memory_fd.
        // This gives us `get_memory_fd_properties`, which queries which Vulkan
        // memory types are compatible with a given dmabuf fd — the call that
        // bridges the compositor-exported fd into `VkImportMemoryFdInfoKHR`.
        let external_memory_fd = if supports_dma_buf_import {
            Some(ash::khr::external_memory_fd::Device::new(&instance, &device))
        } else {
            None
        };

        Ok(Self {
            entry,
            instance,
            physical_device,
            device,
            graphics_queue_family,
            graphics_queue,
            supports_dma_buf_import,
            supports_drm_format_modifier,
            external_memory_fd,
            dmabuf_src_queue_family,
            device_name,
            surface,
            surface_loader,
        })
    }

    /// `true` when the device has everything needed for zero-copy dmabuf
    /// import: `VK_KHR_external_memory_fd`, `VK_EXT_external_memory_dma_buf`,
    /// and `VK_EXT_image_drm_format_modifier` are all present and enabled.
    pub fn supports_zero_copy_capture(&self) -> bool {
        self.supports_dma_buf_import && self.supports_drm_format_modifier
    }
}

// ---------------------------------------------------------------------------
// DMA-BUF → Vulkan import
// ---------------------------------------------------------------------------

/// A Vulkan image + memory + view imported from a dmabuf fd. The fd's
/// ownership transferred to Vulkan on successful import — do NOT close
/// it separately.
///
/// Lifetime: one captured frame → one `DmabufImportedImage` → sample it
/// → retire (destroy) after the GPU fence from the render that first
/// referenced it has signalled.
pub struct DmabufImportedImage {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub view: vk::ImageView,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub opaque_alpha: bool,
    pub y_invert: bool,
}

/// Maps the DRM fourcc formats commonly used by wlr-screencopy to their
/// Vulkan equivalents. This is deliberately a starting point — for XRGB vs ARGB,
/// alpha may be absent or semantically opaque. The shader should force
/// alpha to 1.0 for XRGB/XBGR source formats (see `opaque_alpha` on
/// `DmabufImportedImage`).
pub fn drm_to_vk_format(drm_format: u32) -> vk::Format {
    match drm_format {
        crate::actors::ingress::DRM_FORMAT_XRGB8888
        | crate::actors::ingress::DRM_FORMAT_ARGB8888 => vk::Format::B8G8R8A8_UNORM,
        crate::actors::ingress::DRM_FORMAT_XBGR8888
        | crate::actors::ingress::DRM_FORMAT_ABGR8888 => vk::Format::R8G8B8A8_UNORM,
        // YUYV exported as two UNORM bytes. The patch shader reads .r as Y
        // and .g as the shared chroma. GR88 would swap those and is not used.
        crate::actors::ingress::DRM_FORMAT_RG88 => vk::Format::R8G8_UNORM,
        _ => vk::Format::UNDEFINED,
    }
}

impl VulkanContext {
    /// Imports a dmabuf into a sampled `VkImage`.
    ///
    /// This is the core of the one-copy capture pipeline:
    ///
    /// 1. Build `VkImageDrmFormatModifierExplicitCreateInfoEXT` with the
    ///    per-plane layout (stride, offset) from the export protocol.
    /// 2. Chain `VkExternalMemoryImageCreateInfo` with
    ///    `VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT`.
    /// 3. Create the `VkImage` with
    ///    `VK_IMAGE_TILING_DRM_FORMAT_MODIFIER_EXT`.
    /// 4. Query memory requirements via
    ///    `vkGetImageMemoryRequirements2`.
    /// 5. Query compatible memory types via
    ///    `vkGetMemoryFdPropertiesKHR`.
    /// 6. Allocate `VkDeviceMemory` with `VkImportMemoryFdInfoKHR` +
    ///    `VkMemoryDedicatedAllocateInfo`.
    /// 7. `vkBindImageMemory`.
    /// 8. Create the `VkImageView`.
    ///
    /// On success, the fd's ownership has transferred to Vulkan. On
    /// failure, the fd is still owned by `DmabufFrame` and will be closed
    /// when it drops.
    ///
    /// In the screencopy-dmabuf path, the dmabuf is client-allocated
    /// via GBM and the compositor has copied the frame into it.
    pub fn import_dmabuf(&self, frame: &DmabufFrame) -> Result<DmabufImportedImage, String> {
        let device = &self.device;
        let fd_loader = self
            .external_memory_fd
            .as_ref()
            .ok_or("VK_KHR_external_memory_fd not enabled — cannot import dmabuf")?;

        if !frame.frame_ready_for_import() {
            return Err("dmabuf frame not ready (no planes)".into());
        }

        let vk_format = drm_to_vk_format(frame.drm_format);
        if vk_format == vk::Format::UNDEFINED {
            eprintln!(
                "[dmabuf_import] unsupported DRM format 0x{:08X}",
                frame.drm_format
            );
            return Err(format!("unsupported DRM format 0x{:08X}", frame.drm_format));
        }

        // Initial constraint: accept only one exported object / one fd.
        if frame.planes.len() != 1 {
            eprintln!(
                "[dmabuf_import] multi-object export ({} planes) — \
                 not yet supported",
                frame.planes.len()
            );
            return Err(format!("multi-plane import not supported (got {} planes)", frame.planes.len()));
        }

        let plane = &frame.planes[0];
        // Vulkan takes ownership of the fd passed to vkAllocateMemory and
        // closes it. Dup first so `DmabufFrame`'s OwnedFd can still close
        // the original without a double-close (which drops the GEM handle
        // out from under the imported image).
        let import_fd = unsafe { libc::dup(plane.fd.as_raw_fd()) };
        if import_fd < 0 {
            return Err(format!(
                "dup(dmabuf fd) failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // --- Build the per-plane layout for the modifier create-info chain ---
        let subresource_layout = vk::SubresourceLayout {
            offset: plane.offset as vk::DeviceSize,
            size: 0, // Vulkan computes this; 0 is accepted for explicit layouts
            row_pitch: plane.stride as vk::DeviceSize,
            array_pitch: 0,
            depth_pitch: 0,
        };
        let plane_layouts = [subresource_layout];

        // --- Create the VkImage with DRM format modifier + external memory ---
        // pNext chain: ImageCreateInfo → ExternalMemoryImageCreateInfo →
        //               ImageDrmFormatModifierExplicitCreateInfoEXT
        // `push_next` only exists on root structs; extension structs chain
        // via their public `p_next` field.
        let modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(frame.modifier)
            .plane_layouts(&plane_layouts);

        let mut external_image_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        external_image_info.p_next = &modifier_info as *const _ as *const c_void;

        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk_format)
            .extent(vk::Extent3D {
                width: frame.width,
                height: frame.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            // Sampled only. TRANSFER_* is not in every modifier's usage
            // set, and this image is never copied — the shader reads it.
            .usage(vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external_image_info);

        let image = unsafe { device.create_image(&image_info, None) }
            .map_err(|e| {
                eprintln!(
                    "[dmabuf_import] vkCreateImage failed: {e:?} — format={:?} {}x{} modifier=0x{:016X}",
                    vk_format, frame.width, frame.height, frame.modifier
                );
                unsafe { libc::close(import_fd) };
                format!("vkCreateImage (dmabuf) failed: {e:?}")
            })?;

        // --- Query memory requirements (v2 for modifier-aware results) ---
        let mut mem_req2 = vk::MemoryRequirements2::default();
        let req_info = vk::ImageMemoryRequirementsInfo2::default().image(image);
        unsafe {
            device.get_image_memory_requirements2(&req_info, &mut mem_req2);
        }

        // --- Query which memory types can import this fd ---
        let mut fd_props = vk::MemoryFdPropertiesKHR::default();
        unsafe {
            fd_loader
                .get_memory_fd_properties(
                    vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                    import_fd,
                    &mut fd_props,
                )
                .map_err(|e| {
                    eprintln!(
                        "[dmabuf_import] vkGetMemoryFdPropertiesKHR failed: {e:?} — \
                         format=0x{:08X} modifier=0x{:016X}",
                        frame.drm_format, frame.modifier
                    );
                    libc::close(import_fd);
                    device.destroy_image(image, None);
                    format!("vkGetMemoryFdPropertiesKHR failed: {e:?}")
                })?;
        }

        let compatible_types =
            mem_req2.memory_requirements.memory_type_bits & fd_props.memory_type_bits;
        if compatible_types == 0 {
            eprintln!(
                "[dmabuf_import] no compatible memory type — \
                 req_bits=0x{:08X} fd_bits=0x{:08X} format=0x{:08X} modifier=0x{:016X}",
                mem_req2.memory_requirements.memory_type_bits,
                fd_props.memory_type_bits,
                frame.drm_format,
                frame.modifier
            );
            unsafe {
                libc::close(import_fd);
                device.destroy_image(image, None);
            }
            return Err("no compatible Vulkan memory type for dmabuf fd".into());
        }

        // --- Choose a memory type from the compatible set ---
        let memory_type_index = choose_memory_type(self.physical_device, &self.instance, compatible_types, vk::MemoryPropertyFlags::empty())
            .ok_or_else(|| {
                eprintln!("[dmabuf_import] choose_memory_type found no match");
                unsafe {
                    libc::close(import_fd);
                    device.destroy_image(image, None);
                }
                "no suitable memory type for dmabuf import".to_string()
            })?;

        // --- Allocate memory with fd import + dedicated allocation ---
        // pNext chain: MemoryAllocateInfo → MemoryDedicatedAllocateInfo →
        //               ImportMemoryFdInfoKHR
        let import_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(import_fd);

        let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default()
            .image(image);
        dedicated_info.p_next = &import_info as *const _ as *const c_void;

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_req2.memory_requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut dedicated_info);

        let memory = unsafe { device.allocate_memory(&alloc_info, None) }.map_err(|e| {
            eprintln!(
                "[dmabuf_import] vkAllocateMemory failed: {e:?} — \
                 size={} type_idx={} format=0x{:08X} modifier=0x{:016X}",
                mem_req2.memory_requirements.size, memory_type_index,
                frame.drm_format, frame.modifier
            );
            // On failure the implementation does not take the fd.
            unsafe {
                libc::close(import_fd);
                device.destroy_image(image, None);
            }
            format!("vkAllocateMemory (dmabuf) failed: {e:?}")
        })?;

        // --- Bind image memory ---
        unsafe { device.bind_image_memory(image, memory, 0) }.map_err(|e| {
            eprintln!("[dmabuf_import] vkBindImageMemory failed: {e:?}");
            unsafe {
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            format!("vkBindImageMemory (dmabuf) failed: {e:?}")
        })?;

        // SUCCESS: the driver owns `import_fd` and closes it. The original
        // fd in `DmabufFrame` is a separate dup and closes on drop. The
        // imported VkDeviceMemory holds the GEM object either way.

        // --- Create image view ---
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk_format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let view = unsafe { device.create_image_view(&view_info, None) }.map_err(|e| {
            eprintln!("[dmabuf_import] vkCreateImageView failed: {e:?}");
            unsafe {
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            format!("vkCreateImageView (dmabuf) failed: {e:?}")
        })?;

        Ok(DmabufImportedImage {
            image,
            memory,
            view,
            format: vk_format,
            width: frame.width,
            height: frame.height,
            opaque_alpha: frame.opaque_alpha(),
            y_invert: frame.y_invert,
        })
    }

    /// Acquire a compositor-written dmabuf so the fragment shader can
    /// sample it.
    ///
    /// `oldLayout = UNDEFINED` tells the driver it may discard texels.
    /// The screencopy copy already landed in this memory, so the acquire
    /// must preserve them: `GENERAL` (the layout external producers leave
    /// an image in) → `SHADER_READ_ONLY_OPTIMAL`, with the source queue
    /// family outside this device.
    ///
    /// Call this once before the first draw that samples the imported
    /// image, in the same command buffer as the render.
    pub fn record_dmabuf_transition(
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        src_queue_family: u32,
        dst_queue_family: u32,
    ) {
        let barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(src_queue_family)
            .dst_queue_family_index(dst_queue_family)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&barrier),
            );
        }
    }

    /// Hand a sampled dmabuf back to the device that writes it (the webcam).
    /// The previous acquire left the image in `SHADER_READ_ONLY_OPTIMAL`.
    /// `GENERAL` is what a foreign producer writes, and the contents stay:
    /// this is not an `UNDEFINED` discard. Call it after the dispatch that
    /// sampled the image, and do not queue the buffer again until the
    /// fence for this command buffer has signaled.
    pub fn record_dmabuf_release(
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        src_queue_family: u32,
        dst_queue_family: u32,
    ) {
        let barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::empty())
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(src_queue_family)
            .dst_queue_family_index(dst_queue_family)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&barrier),
            );
        }
    }

    /// Destroys a `DmabufImportedImage`. Must only be called after the
    /// command buffers that sample it have completed (i.e. after the
    /// per-frame fence has signalled).
    pub fn destroy_dmabuf_import(&self, imported: &mut DmabufImportedImage) {
        unsafe {
            self.device.destroy_image_view(imported.view, None);
            self.device.destroy_image(imported.image, None);
            self.device.free_memory(imported.memory, None);
        }
        imported.image = vk::Image::null();
        imported.memory = vk::DeviceMemory::null();
        imported.view = vk::ImageView::null();
    }
}

/// Extension trait so `DmabufFrame` can self-check readiness without
/// leaking the internal `planes` representation.
trait DmabufFrameExt {
    fn frame_ready_for_import(&self) -> bool;
}

impl DmabufFrameExt for DmabufFrame {
    fn frame_ready_for_import(&self) -> bool {
        !self.planes.is_empty()
            && self.width > 0
            && self.height > 0
            && self.modifier != DRM_FORMAT_MOD_INVALID
    }
}

/// Picks a memory type from `compatible_bits` (a bitmask of allowed types)
/// that also has all of the requested `required_flags`. Returns the index
/// or `None` if no type satisfies both constraints.
fn choose_memory_type(
    physical_device: vk::PhysicalDevice,
    instance: &ash::Instance,
    compatible_bits: u32,
    required_flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    for i in 0..props.memory_type_count {
        if (compatible_bits & (1 << i)) != 0
            && props.memory_types[i as usize]
                .property_flags
                .contains(required_flags)
        {
            return Some(i);
        }
    }
    None
}

/// Prefer a discrete GPU with a graphics queue; fall back to integrated,
/// virtual, then anything else that can actually draw. `[0]` is often an
/// iGPU even when a dGPU is present.
fn pick_physical_device(
    instance: &ash::Instance,
    devices: &[vk::PhysicalDevice],
    surface_loader: Option<&ash::khr::surface::Instance>,
    surface: Option<vk::SurfaceKHR>,
) -> Result<(vk::PhysicalDevice, u32, String), String> {
    let score = |ty: vk::PhysicalDeviceType| -> i32 {
        match ty {
            vk::PhysicalDeviceType::DISCRETE_GPU => 4,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
            vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
            vk::PhysicalDeviceType::CPU => 1,
            _ => 0,
        }
    };

    let mut best: Option<(i32, vk::PhysicalDevice, u32, String)> = None;
    for &physical_device in devices {
        let props = unsafe { instance.get_physical_device_properties(physical_device) };
        let device_name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let Some(graphics_queue_family) = queue_families
            .iter()
            .enumerate()
            .find(|(_, qf)| qf.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|(i, _)| i as u32)
        else {
            continue;
        };
        if let (Some(loader), Some(surf)) = (surface_loader, surface) {
            let present = unsafe {
                loader.get_physical_device_surface_support(physical_device, graphics_queue_family, surf)
            }
            .unwrap_or(false);
            if !present {
                continue;
            }
        }
        let s = score(props.device_type);
        if best.as_ref().map(|(best_s, ..)| s > *best_s).unwrap_or(true) {
            best = Some((s, physical_device, graphics_queue_family, device_name));
        }
    }
    best.map(|(_, dev, family, name)| (dev, family, name))
        .ok_or_else(|| "no Vulkan physical device with a graphics queue family".into())
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_device(None);
            if let (Some(surface), Some(loader)) = (self.surface.take(), self.surface_loader.as_ref()) {
                loader.destroy_surface(surface, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
