//! Atlas of static screen rectangles. The copy reads the imported screen
//! image and writes one tile. Nothing is mapped back to the CPU.

use crate::avatar::appearance::{
    PatchJob, RegionGpu, SplatPatchGpu, ATLAS_H, ATLAS_W, MAX_PATCH_SPLATS, MAX_REGIONS, TILE,
};
use crate::vulkan::VulkanContext;
use ash::vk;
use std::ffi::CStr;

const COMP_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/screen_patch.comp.spv"));

#[repr(C)]
struct PatchPush {
    src_rect: [f32; 4],
    screen_size: [f32; 2],
    atlas_size: [f32; 2],
    tile_origin: [i32; 2],
    tile_size: i32,
    y_invert: f32,
}

pub struct CaptureView {
    pub view: vk::ImageView,
    pub width: u32,
    pub height: u32,
    pub y_invert: bool,
}

pub struct PatchBind {
    pub patches: vk::Buffer,
    pub patch_bytes: vk::DeviceSize,
    pub regions: vk::Buffer,
    pub region_bytes: vk::DeviceSize,
    pub atlas_view: vk::ImageView,
    pub atlas_sampler: vk::Sampler,
}

pub struct ScreenPatches {
    atlas: vk::Image,
    atlas_mem: vk::DeviceMemory,
    atlas_view: vk::ImageView,
    atlas_sampler: vk::Sampler,
    screen_sampler: vk::Sampler,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    patch_buf: vk::Buffer,
    patch_mem: vk::DeviceMemory,
    patch_ptr: *mut SplatPatchGpu,
    region_buf: vk::Buffer,
    region_mem: vk::DeviceMemory,
    region_ptr: *mut RegionGpu,
    cleared: bool,
    patches_uploaded: bool,
    show_bake: bool,
    jobs: Vec<PatchJob>,
}

impl ScreenPatches {
    pub fn new(vk: &VulkanContext) -> Result<Self, String> {
        let features = unsafe { vk.instance.get_physical_device_features(vk.physical_device) };
        if features.shader_storage_image_extended_formats == 0 {
            return Err("shaderStorageImageExtendedFormats is required for the screen-tile atlas".into());
        }
        let fmt = unsafe {
            vk.instance.get_physical_device_format_properties(vk.physical_device, vk::Format::R8G8B8A8_UNORM)
        };
        let need = vk::FormatFeatureFlags::SAMPLED_IMAGE
            | vk::FormatFeatureFlags::STORAGE_IMAGE
            | vk::FormatFeatureFlags::TRANSFER_DST;
        if !fmt.optimal_tiling_features.contains(need) {
            return Err("R8G8B8A8 atlas cannot be sampled, stored, and cleared".into());
        }

        let device = &vk.device;
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .extent(vk::Extent3D { width: ATLAS_W, height: ATLAS_H, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::STORAGE)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let atlas = unsafe { device.create_image(&image_info, None) }
            .map_err(|e| format!("atlas image: {e:?}"))?;
        let img_req = unsafe { device.get_image_memory_requirements(atlas) };
        let atlas_mem = alloc(vk, img_req, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        unsafe { device.bind_image_memory(atlas, atlas_mem, 0) }.map_err(|e| format!("atlas bind: {e:?}"))?;
        let atlas_view = view(device, atlas, vk::Format::R8G8B8A8_UNORM)?;
        let atlas_sampler = sampler(device)?;
        let screen_sampler = sampler(device)?;

        let (patch_buf, patch_mem, patch_ptr) = host_buffer(
            vk,
            (MAX_PATCH_SPLATS * std::mem::size_of::<SplatPatchGpu>()) as vk::DeviceSize,
        )?;
        let (region_buf, region_mem, region_ptr_u8) = host_buffer(
            vk,
            (MAX_REGIONS * std::mem::size_of::<RegionGpu>()) as vk::DeviceSize,
        )?;
        let region_ptr = region_ptr_u8 as *mut RegionGpu;
        unsafe {
            std::ptr::write_bytes(region_ptr, 0, MAX_REGIONS);
            std::ptr::write_bytes(patch_ptr as *mut u8, 0, MAX_PATCH_SPLATS * std::mem::size_of::<SplatPatchGpu>());
        }

        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        let dsl_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let set_layout = unsafe { device.create_descriptor_set_layout(&dsl_info, None) }
            .map_err(|e| format!("patch set layout: {e:?}"))?;
        let pool_sizes = [
            vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: 1 },
            vk::DescriptorPoolSize { ty: vk::DescriptorType::STORAGE_IMAGE, descriptor_count: 1 },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo::default().max_sets(1).pool_sizes(&pool_sizes);
        let pool = unsafe { device.create_descriptor_pool(&pool_info, None) }
            .map_err(|e| format!("patch pool: {e:?}"))?;
        let layouts = [set_layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&layouts);
        let set = unsafe { device.allocate_descriptor_sets(&alloc_info) }
            .map_err(|e| format!("patch set: {e:?}"))?[0];

        let atlas_info = [vk::DescriptorImageInfo::default()
            .image_view(atlas_view)
            .image_layout(vk::ImageLayout::GENERAL)];
        let atlas_write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .image_info(&atlas_info);
        unsafe { device.update_descriptor_sets(&[atlas_write], &[]) };

        let push = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(std::mem::size_of::<PatchPush>() as u32);
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(std::slice::from_ref(&push));
        let layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
            .map_err(|e| format!("patch pipeline layout: {e:?}"))?;
        let module = shader_module(device, COMP_SPV)?;
        let entry = CStr::from_bytes_with_nul(b"main\0").unwrap();
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(module)
            .name(entry);
        let create = vk::ComputePipelineCreateInfo::default().stage(stage).layout(layout);
        let pipeline = unsafe { device.create_compute_pipelines(vk::PipelineCache::null(), &[create], None) }
            .map_err(|(_, e)| format!("patch pipeline: {e:?}"))?[0];
        unsafe { device.destroy_shader_module(module, None) };

        Ok(Self {
            atlas,
            atlas_mem,
            atlas_view,
            atlas_sampler,
            screen_sampler,
            layout,
            pipeline,
            set_layout,
            pool,
            set,
            patch_buf,
            patch_mem,
            patch_ptr: patch_ptr as *mut SplatPatchGpu,
            region_buf,
            region_mem,
            region_ptr,
            cleared: false,
            patches_uploaded: false,
            show_bake: false,
            jobs: Vec::new(),
        })
    }

    pub fn bind(&self) -> PatchBind {
        PatchBind {
            patches: self.patch_buf,
            patch_bytes: (MAX_PATCH_SPLATS * std::mem::size_of::<SplatPatchGpu>()) as vk::DeviceSize,
            regions: self.region_buf,
            region_bytes: (MAX_REGIONS * std::mem::size_of::<RegionGpu>()) as vk::DeviceSize,
            atlas_view: self.atlas_view,
            atlas_sampler: self.atlas_sampler,
        }
    }

    pub fn show_bake(&self) -> bool {
        self.show_bake
    }

    pub fn enqueue(&mut self, job: PatchJob) {
        self.jobs.push(job);
    }

    pub fn sync(&mut self, appearance: &crate::avatar::appearance::Appearance) {
        self.show_bake = appearance.show_bake;
        if !self.patches_uploaded && !appearance.patches.is_empty() {
            let gpu = appearance.patch_gpu();
            unsafe {
                std::ptr::copy_nonoverlapping(gpu.as_ptr(), self.patch_ptr, gpu.len());
            }
            self.patches_uploaded = true;
        }
        let regions = appearance.region_uniforms();
        unsafe {
            std::ptr::copy_nonoverlapping(regions.as_ptr(), self.region_ptr, MAX_REGIONS);
        }
    }

    /// Clear the atlas once, then copy any queued rectangles from `capture`.
    /// Jobs stay queued when the screen image is not ready yet.
    pub fn record(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, capture: Option<&CaptureView>) {
        if !self.cleared {
            self.clear(device, cmd);
            self.cleared = true;
        }
        let Some(capture) = capture else { return };
        if self.jobs.is_empty() {
            return;
        }
        let screen_info = [vk::DescriptorImageInfo::default()
            .sampler(self.screen_sampler)
            .image_view(capture.view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&screen_info);
        unsafe { device.update_descriptor_sets(&[write], &[]) };

        image_barrier(
            device,
            cmd,
            self.atlas,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageLayout::GENERAL,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_READ,
            vk::AccessFlags::SHADER_WRITE,
        );
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::COMPUTE, self.layout, 0, &[self.set], &[]);
        }
        for job in self.jobs.drain(..) {
            println!(
                "[appearance] region {} page {} <- {},{} {}x{}",
                job.region, job.page, job.rect.x, job.rect.y, job.rect.w, job.rect.h
            );
            let push = PatchPush {
                src_rect: [job.rect.x as f32, job.rect.y as f32, job.rect.w as f32, job.rect.h as f32],
                screen_size: [capture.width as f32, capture.height as f32],
                atlas_size: [ATLAS_W as f32, ATLAS_H as f32],
                tile_origin: [job.region as i32 * TILE as i32, job.page as i32 * TILE as i32],
                tile_size: TILE as i32,
                y_invert: if capture.y_invert { 1.0 } else { 0.0 },
            };
            unsafe {
                device.cmd_push_constants(
                    cmd,
                    self.layout,
                    vk::ShaderStageFlags::COMPUTE,
                    0,
                    std::slice::from_raw_parts(&push as *const PatchPush as *const u8, std::mem::size_of::<PatchPush>()),
                );
                device.cmd_dispatch(cmd, (TILE + 7) / 8, (TILE + 7) / 8, 1);
            }
        }
        image_barrier(
            device,
            cmd,
            self.atlas,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::AccessFlags::SHADER_WRITE,
            vk::AccessFlags::SHADER_READ,
        );
    }

    fn clear(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        image_barrier(
            device,
            cmd,
            self.atlas,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::empty(),
            vk::AccessFlags::TRANSFER_WRITE,
        );
        let clear = vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] };
        let range = color_range();
        unsafe {
            device.cmd_clear_color_image(cmd, self.atlas, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &clear, &[range]);
        }
        image_barrier(
            device,
            cmd,
            self.atlas,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::SHADER_READ,
        );
    }

    pub fn destroy(&mut self, device: &ash::Device) {
        if self.pipeline == vk::Pipeline::null() {
            return;
        }
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
            device.destroy_sampler(self.atlas_sampler, None);
            device.destroy_sampler(self.screen_sampler, None);
            device.destroy_image_view(self.atlas_view, None);
            device.destroy_image(self.atlas, None);
            device.free_memory(self.atlas_mem, None);
            device.unmap_memory(self.patch_mem);
            device.destroy_buffer(self.patch_buf, None);
            device.free_memory(self.patch_mem, None);
            device.unmap_memory(self.region_mem);
            device.destroy_buffer(self.region_buf, None);
            device.free_memory(self.region_mem, None);
        }
        self.pipeline = vk::Pipeline::null();
    }
}

fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}

fn image_barrier(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    old: vk::ImageLayout,
    new: vk::ImageLayout,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
) {
    let barrier = vk::ImageMemoryBarrier::default()
        .src_access_mask(src_access)
        .dst_access_mask(dst_access)
        .old_layout(old)
        .new_layout(new)
        .image(image)
        .subresource_range(color_range());
    unsafe {
        device.cmd_pipeline_barrier(cmd, src_stage, dst_stage, vk::DependencyFlags::empty(), &[], &[], &[barrier]);
    }
}

fn view(device: &ash::Device, image: vk::Image, format: vk::Format) -> Result<vk::ImageView, String> {
    let info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(color_range());
    unsafe { device.create_image_view(&info, None) }.map_err(|e| format!("atlas view: {e:?}"))
}

fn sampler(device: &ash::Device) -> Result<vk::Sampler, String> {
    let info = vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
    unsafe { device.create_sampler(&info, None) }.map_err(|e| format!("atlas sampler: {e:?}"))
}

fn host_buffer(
    vk: &VulkanContext,
    size: vk::DeviceSize,
) -> Result<(vk::Buffer, vk::DeviceMemory, *mut u8), String> {
    let info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { vk.device.create_buffer(&info, None) }.map_err(|e| format!("patch buffer: {e:?}"))?;
    let req = unsafe { vk.device.get_buffer_memory_requirements(buffer) };
    let memory = alloc(
        vk,
        req,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    unsafe { vk.device.bind_buffer_memory(buffer, memory, 0) }.map_err(|e| format!("patch buffer bind: {e:?}"))?;
    let ptr = unsafe { vk.device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty()) }
        .map_err(|e| format!("patch buffer map: {e:?}"))?;
    Ok((buffer, memory, ptr as *mut u8))
}

fn alloc(
    vk: &VulkanContext,
    req: vk::MemoryRequirements,
    flags: vk::MemoryPropertyFlags,
) -> Result<vk::DeviceMemory, String> {
    let props = unsafe { vk.instance.get_physical_device_memory_properties(vk.physical_device) };
    let mut index = None;
    for i in 0..props.memory_type_count {
        if req.memory_type_bits & (1 << i) != 0 && props.memory_types[i as usize].property_flags.contains(flags) {
            index = Some(i);
            break;
        }
    }
    let memory_type_index = index.ok_or("no memory type for screen tiles")?;
    let info = vk::MemoryAllocateInfo::default()
        .allocation_size(req.size)
        .memory_type_index(memory_type_index);
    unsafe { vk.device.allocate_memory(&info, None) }.map_err(|e| format!("patch memory: {e:?}"))
}

fn shader_module(device: &ash::Device, bytes: &[u8]) -> Result<vk::ShaderModule, String> {
    let code = ash::util::read_spv(&mut std::io::Cursor::new(bytes)).map_err(|e| format!("patch spirv: {e}"))?;
    let info = vk::ShaderModuleCreateInfo::default().code(&code);
    unsafe { device.create_shader_module(&info, None) }.map_err(|e| format!("patch module: {e:?}"))
}
