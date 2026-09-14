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
    pub supports_dma_buf_import: bool,
    pub device_name: String,
}

impl VulkanContext {
    pub fn init() -> Result<Self, String> {
        // SAFETY: `Entry::load` dynamically loads the system Vulkan loader.
        // This is the one place in the whole daemon where "no GPU/driver
        // present" is expected and handled, rather than a bug.
        let entry = unsafe { Entry::load() }
            .map_err(|e| format!("no Vulkan loader found on this system: {e}"))?;

        let app_info = vk::ApplicationInfo::default()
            .application_name(c"pachakutech-presence-daemon")
            .api_version(vk::API_VERSION_1_1); // AHardwareBuffer/dma_buf import needs >= 1.1

        let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);
        let instance = unsafe { entry.create_instance(&create_info, None) }
            .map_err(|e| format!("vkCreateInstance failed: {e:?}"))?;

        let physical_devices = unsafe { instance.enumerate_physical_devices() }
            .map_err(|e| format!("failed to enumerate physical devices: {e:?}"))?;
        if physical_devices.is_empty() {
            unsafe { instance.destroy_instance(None) };
            return Err("no Vulkan-capable physical devices found".into());
        }

        // Pick the first device with a graphics-capable queue family. A real
        // build should prefer discrete GPUs and check for the extensions
        // below explicitly rather than just picking [0], but this is enough
        // to prove the pipeline end to end.
        let physical_device = physical_devices[0];

        let props = unsafe { instance.get_physical_device_properties(physical_device) };
        let device_name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) }
            .to_string_lossy()
            .into_owned();

        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let graphics_queue_family = queue_families
            .iter()
            .enumerate()
            .find(|(_, qf)| qf.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|(i, _)| i as u32)
            .ok_or("selected physical device has no graphics queue family")?;

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
        // when they're not (e.g. running this in a VM or a sandbox with a
        // software rasterizer) rather than refusing to start.
        let mut enabled_extensions: Vec<*const i8> = Vec::new();
        if supports_dma_buf_import {
            enabled_extensions.push(ash::khr::external_memory_fd::NAME.as_ptr());
            enabled_extensions.push(ash::ext::external_memory_dma_buf::NAME.as_ptr());
        }

        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&enabled_extensions);

        let device = unsafe { instance.create_device(physical_device, &device_create_info, None) }
            .map_err(|e| format!("vkCreateDevice failed: {e:?}"))?;

        Ok(Self {
            entry,
            instance,
            physical_device,
            device,
            graphics_queue_family,
            supports_dma_buf_import,
            device_name,
        })
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}
