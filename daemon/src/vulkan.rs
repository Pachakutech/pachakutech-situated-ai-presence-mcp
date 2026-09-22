//! Owns the GPU. Deliberately minimal for now: create an Instance, pick a
//! physical device, report whether it can do the zero-copy dma_buf import
//! path the architecture depends on, and create a logical Device + queue.
//! No swapchain, no rendering yet — that's the next layer up, and it's
//! yours to build (see README.md "What's next").

use ash::{vk, Entry};
use std::ffi::CStr;

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

        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&enabled_extensions);

        let device = unsafe { instance.create_device(physical_device, &device_create_info, None) }
            .map_err(|e| format!("vkCreateDevice failed: {e:?}"))?;
        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family, 0) };

        Ok(Self {
            entry,
            instance,
            physical_device,
            device,
            graphics_queue_family,
            graphics_queue,
            supports_dma_buf_import,
            device_name,
            surface,
            surface_loader,
        })
    }
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
