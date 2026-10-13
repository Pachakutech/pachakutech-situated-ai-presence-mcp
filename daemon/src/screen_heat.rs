//! Coarse heat grid from the desktop capture. The picture is sampled on the
//! GPU. The CPU receives the cell heats only, after the present fence.

use crate::placement::{HeatGrid, DISC_PX};
use crate::screen_patch::CaptureView;
use crate::vulkan::VulkanContext;
use ash::vk;
use std::ffi::CStr;

const COMP_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/screen_heat.comp.spv"));
const SEAT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/screen_heat_seat.comp.spv"));

const CELL_PX: u32 = 40;
const MAX_COLS: u32 = 192;
const MAX_ROWS: u32 = 108;
const MAX_CELLS: u32 = MAX_COLS * MAX_ROWS;
/// One vec4 past the cells. The seat shader eases the iris opening here.
/// The CPU copy never reads it.
const SEAT_INDEX: u32 = MAX_CELLS;
const _: () = assert!(SEAT_INDEX == 192 * 108);

/// Tunable from here. The shader reads them as push constants.
const W_TEXT: f32 = 1.0;
/// Left at zero. A highlight with nothing written in it must not outrank
/// real text, and raising this would do that. The term stays in the shader.
const W_SELECT: f32 = 0.0;
/// Measured, and kept, so a later actor can use window lines as
/// scaffolding. Barely weighed here: a framed empty box should still
/// be a place to sit.
const W_EDGE: f32 = 0.04;
const W_COLOR: f32 = 0.18;
const W_MOTION: f32 = 1.2;
/// Letters changing in a cell that already looks like text. The mean
/// brightness barely moves when grey type is rewritten, so this is the
/// term that makes a live paragraph the hot end of the field.
const W_CHANGING: f32 = 8.0;

#[repr(C)]
struct HeatPush {
    screen_size: [f32; 2],
    cell_size: f32,
    y_invert: f32,
    cols: u32,
    rows: u32,
    have_prev: u32,
    _pad: u32,
    weights0: [f32; 4],
    weights1: [f32; 4],
    mask_a: [f32; 4],
    mask_b: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<HeatPush>() == 96);
const _: () = assert!(std::mem::offset_of!(HeatPush, weights0) == 32);
const _: () = assert!(std::mem::offset_of!(HeatPush, mask_a) == 64);
const _: () = assert!(std::mem::offset_of!(HeatPush, mask_b) == 80);

struct Written {
    cols: u32,
    rows: u32,
    cell_px: u32,
    screen_w: u32,
    screen_h: u32,
    mask_a: (i32, i32),
    mask_b: (i32, i32),
}

pub struct ScreenHeat {
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    seat_pipeline: vk::Pipeline,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    /// Same buffer, fragment-readable, for the iris opening.
    lens_set_layout: vk::DescriptorSetLayout,
    lens_set: vk::DescriptorSet,
    sampler: vk::Sampler,
    buf: vk::Buffer,
    mem: vk::DeviceMemory,
    ptr: *mut f32,
    disc: (i32, i32),
    prev_disc: (i32, i32),
    have_prev: bool,
    cols: u32,
    rows: u32,
    cell_px: u32,
    written: Option<Written>,
    ready: Option<HeatGrid>,
    logged: bool,
    logged_fail: bool,
}

impl ScreenHeat {
    pub fn new(vk: &VulkanContext) -> Result<Self, String> {
        let device = &vk.device;
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        let dsl_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let set_layout = unsafe { device.create_descriptor_set_layout(&dsl_info, None) }
            .map_err(|e| format!("heat set layout: {e:?}"))?;
        let pool_sizes = [
            vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: 1 },
            vk::DescriptorPoolSize { ty: vk::DescriptorType::STORAGE_BUFFER, descriptor_count: 2 },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo::default().max_sets(2).pool_sizes(&pool_sizes);
        let pool = unsafe { device.create_descriptor_pool(&pool_info, None) }
            .map_err(|e| format!("heat pool: {e:?}"))?;
        let layouts = [set_layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&layouts);
        let set = unsafe { device.allocate_descriptor_sets(&alloc_info) }
            .map_err(|e| format!("heat set: {e:?}"))?[0];

        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
        let sampler = unsafe { device.create_sampler(&sampler_info, None) }
            .map_err(|e| format!("heat sampler: {e:?}"))?;

        let slots = (MAX_CELLS + 1) as vk::DeviceSize;
        let bytes = slots * std::mem::size_of::<[f32; 4]>() as vk::DeviceSize;
        let buf_info = vk::BufferCreateInfo::default()
            .size(bytes)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buf = unsafe { device.create_buffer(&buf_info, None) }.map_err(|e| format!("heat buffer: {e:?}"))?;
        let req = unsafe { device.get_buffer_memory_requirements(buf) };
        let mem = alloc(vk, req, vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT)?;
        unsafe { device.bind_buffer_memory(buf, mem, 0) }.map_err(|e| format!("heat buffer bind: {e:?}"))?;
        let ptr = unsafe { device.map_memory(mem, 0, bytes, vk::MemoryMapFlags::empty()) }
            .map_err(|e| format!("heat buffer map: {e:?}"))? as *mut f32;
        unsafe { std::ptr::write_bytes(ptr, 0, (MAX_CELLS as usize + 1) * 4) };

        let lens_binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let lens_dsl = vk::DescriptorSetLayoutCreateInfo::default().bindings(std::slice::from_ref(&lens_binding));
        let lens_set_layout = unsafe { device.create_descriptor_set_layout(&lens_dsl, None) }
            .map_err(|e| format!("heat lens set layout: {e:?}"))?;
        let lens_layouts = [lens_set_layout];
        let lens_alloc = vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&lens_layouts);
        let lens_set = unsafe { device.allocate_descriptor_sets(&lens_alloc) }
            .map_err(|e| format!("heat lens set: {e:?}"))?[0];
        let lens_buf = [vk::DescriptorBufferInfo::default().buffer(buf).offset(0).range(bytes)];
        let lens_write = vk::WriteDescriptorSet::default()
            .dst_set(lens_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&lens_buf);
        unsafe { device.update_descriptor_sets(&[lens_write], &[]) };

        let buf_info = [vk::DescriptorBufferInfo::default().buffer(buf).offset(0).range(bytes)];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&buf_info);
        unsafe { device.update_descriptor_sets(&[write], &[]) };

        let push = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(std::mem::size_of::<HeatPush>() as u32);
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(std::slice::from_ref(&push));
        let layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
            .map_err(|e| format!("heat pipeline layout: {e:?}"))?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(COMP_SPV)).map_err(|e| format!("heat spirv: {e}"))?;
        let module_info = vk::ShaderModuleCreateInfo::default().code(&code);
        let module = unsafe { device.create_shader_module(&module_info, None) }
            .map_err(|e| format!("heat module: {e:?}"))?;
        let entry = CStr::from_bytes_with_nul(b"main\0").unwrap();
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(module)
            .name(entry);
        let create = vk::ComputePipelineCreateInfo::default().stage(stage).layout(layout);
        let pipeline = unsafe { device.create_compute_pipelines(vk::PipelineCache::null(), &[create], None) }
            .map_err(|(_, e)| format!("heat pipeline: {e:?}"))?[0];
        unsafe { device.destroy_shader_module(module, None) };

        let seat_code = ash::util::read_spv(&mut std::io::Cursor::new(SEAT_SPV)).map_err(|e| format!("heat seat spirv: {e}"))?;
        let seat_module = unsafe { device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&seat_code), None) }
            .map_err(|e| format!("heat seat module: {e:?}"))?;
        let seat_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(seat_module)
            .name(entry);
        let seat_create = vk::ComputePipelineCreateInfo::default().stage(seat_stage).layout(layout);
        let seat_pipeline = unsafe { device.create_compute_pipelines(vk::PipelineCache::null(), &[seat_create], None) }
            .map_err(|(_, e)| format!("heat seat pipeline: {e:?}"))?[0];
        unsafe { device.destroy_shader_module(seat_module, None) };

        Ok(Self {
            layout,
            pipeline,
            seat_pipeline,
            set_layout,
            pool,
            set,
            lens_set_layout,
            lens_set,
            sampler,
            buf,
            mem,
            ptr,
            disc: (0, 0),
            prev_disc: (0, 0),
            have_prev: false,
            cols: 0,
            rows: 0,
            cell_px: CELL_PX,
            written: None,
            ready: None,
            logged: false,
            logged_fail: false,
        })
    }

    pub fn set_disc(&mut self, x: i32, y: i32) {
        self.disc = (x, y);
    }

    /// Copy the grid the previous dispatch wrote. Call after the fence for
    /// that command buffer has signaled, and before the next dispatch.
    pub fn collect(&mut self) {
        let Some(written) = self.written.take() else { return };
        let count = (written.cols * written.rows) as usize;
        let mut heat = Vec::with_capacity(count);
        unsafe {
            for i in 0..count {
                heat.push(*self.ptr.add(i * 4));
            }
        }
        if !self.logged {
            eprintln!(
                "[placement] heat grid {}x{} cell {}px",
                written.cols, written.rows, written.cell_px
            );
            self.logged = true;
        }
        self.ready = Some(HeatGrid {
            cols: written.cols,
            rows: written.rows,
            cell_px: written.cell_px,
            screen_w: written.screen_w,
            screen_h: written.screen_h,
            heat,
            mask_a: written.mask_a,
            mask_b: written.mask_b,
        });
    }

    pub fn take(&mut self) -> Option<HeatGrid> {
        self.ready.take()
    }

    /// Sample `view` into the grid. The image is already in
    /// `SHADER_READ_ONLY_OPTIMAL` (or the transition is earlier in this
    /// command buffer). One failure is logged; the layer simply stays put.
    pub fn record(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, view: &CaptureView) -> Result<(), String> {
        if view.width == 0 || view.height == 0 {
            return Ok(());
        }
        let (cols, rows, cell) = grid_shape(view.width, view.height);
        if cols != self.cols || rows != self.rows || cell != self.cell_px {
            self.have_prev = false;
        }
        self.cols = cols;
        self.rows = rows;
        self.cell_px = cell;

        let image_info = [vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(view.view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_info);
        unsafe { device.update_descriptor_sets(&[write], &[]) };

        let mask_a = self.disc;
        let mask_b = if self.have_prev { self.prev_disc } else { self.disc };
        let push = HeatPush {
            screen_size: [view.width as f32, view.height as f32],
            cell_size: cell as f32,
            y_invert: if view.y_invert { 1.0 } else { 0.0 },
            cols,
            rows,
            have_prev: if self.have_prev { 1 } else { 0 },
            _pad: 0,
            weights0: [W_TEXT, W_SELECT, W_EDGE, W_COLOR],
            weights1: [W_MOTION, W_CHANGING, 0.0, 0.0],
            mask_a: [mask_a.0 as f32, mask_a.1 as f32, DISC_PX as f32, DISC_PX as f32],
            mask_b: [mask_b.0 as f32, mask_b.1 as f32, DISC_PX as f32, DISC_PX as f32],
        };
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::COMPUTE, self.layout, 0, &[self.set], &[]);
            device.cmd_push_constants(
                cmd,
                self.layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                std::slice::from_raw_parts((&push as *const HeatPush).cast::<u8>(), std::mem::size_of::<HeatPush>()),
            );
            device.cmd_dispatch(cmd, (cols + 7) / 8, (rows + 7) / 8, 1);
            let seat_barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                .buffer(self.buf)
                .offset(0)
                .size(vk::WHOLE_SIZE);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[seat_barrier],
                &[],
            );
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.seat_pipeline);
            device.cmd_dispatch(cmd, 1, 1, 1);
            let barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ)
                .buffer(self.buf)
                .offset(0)
                .size(vk::WHOLE_SIZE);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[],
                &[barrier],
                &[],
            );
        }
        self.prev_disc = mask_a;
        self.have_prev = true;
        self.written = Some(Written {
            cols,
            rows,
            cell_px: cell,
            screen_w: view.width,
            screen_h: view.height,
            mask_a,
            mask_b,
        });
        Ok(())
    }

    pub fn note_failure(&mut self, err: &str) {
        if !self.logged_fail {
            eprintln!("[placement] heat dispatch failed: {err}");
            self.logged_fail = true;
        }
    }

    pub fn lens_set_layout(&self) -> vk::DescriptorSetLayout {
        self.lens_set_layout
    }

    pub fn lens_set(&self) -> vk::DescriptorSet {
        self.lens_set
    }

    pub fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.seat_pipeline, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.lens_set_layout, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
            device.destroy_sampler(self.sampler, None);
            device.unmap_memory(self.mem);
            device.destroy_buffer(self.buf, None);
            device.free_memory(self.mem, None);
        }
    }
}

fn grid_shape(w: u32, h: u32) -> (u32, u32, u32) {
    let mut cell = CELL_PX.max(1);
    let mut cols = div_up(w, cell);
    let mut rows = div_up(h, cell);
    if cols > MAX_COLS || rows > MAX_ROWS {
        cell = div_up(w, MAX_COLS).max(div_up(h, MAX_ROWS)).max(1);
        cols = div_up(w, cell).min(MAX_COLS);
        rows = div_up(h, cell).min(MAX_ROWS);
    }
    (cols.max(1), rows.max(1), cell)
}

fn div_up(n: u32, d: u32) -> u32 {
    n.saturating_add(d - 1) / d.max(1)
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
    let memory_type_index = index.ok_or("no host-visible memory for the heat grid")?;
    let info = vk::MemoryAllocateInfo::default()
        .allocation_size(req.size)
        .memory_type_index(memory_type_index);
    unsafe { vk.device.allocate_memory(&info, None) }.map_err(|e| format!("heat memory: {e:?}"))
}
