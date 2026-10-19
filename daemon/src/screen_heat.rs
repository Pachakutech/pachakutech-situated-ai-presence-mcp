//! Coarse heat grid from the desktop capture. The picture is sampled on the
//! GPU. The CPU receives the cell heats only, after the present fence.

use crate::placement::{HeatGrid, OBSERVE_PX};
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
/// Three more vec4s are highlight rects (x, y, w, h in heat, luma, cb, cr).
/// The CPU copy never reads any of them.
const SEAT_INDEX: u32 = MAX_CELLS;
const MARK_SLOTS: u32 = 3;
const MARK0: u32 = SEAT_INDEX + 1;
const _: () = assert!(SEAT_INDEX == 192 * 108);
const _: () = assert!(MARK0 == 192 * 108 + 1);

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
/// Highlight interior. Hotter than text, short of a cell whose letters are
/// changing. The shader writes 0 there (the tint is not the page); the copy
/// replaces that 0 with this.
const W_MARK_HOT: f32 = 4.0;
/// Subtracted from the frame just outside the highlight and its margin.
/// About one text-weight, so a blank frame is cooler than blank desktop and
/// a changing paragraph there stays hot.
const W_MARK_COOL: f32 = 1.0;
/// Pixels outside the rect that still count with the highlight, at text weight.
const MARK_MARGIN_PX: f32 = 12.0;
/// Pixels outside that margin that cool. One cell of the 40 px grid, so the
/// ring has a center to land in.
const MARK_FRAME_PX: f32 = 40.0;

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
    disc_a: i32,
    disc_b: i32,
    /// Rects the dispatch masked. The stamp uses these, not a later set.
    marks: [[f32; 4]; 3],
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
    disc_px: i32,
    prev_disc_px: i32,
    /// Up to three highlight rects in screen pixels. Zero width means none.
    marks: [[f32; 4]; 3],
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

        let slots = (MAX_CELLS + 1 + MARK_SLOTS) as vk::DeviceSize;
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
        unsafe { std::ptr::write_bytes(ptr, 0, (MAX_CELLS as usize + 1 + MARK_SLOTS as usize) * 4) };

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
            disc_px: OBSERVE_PX,
            prev_disc_px: OBSERVE_PX,
            marks: [[0.0; 4]; 3],
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

    pub fn set_disc(&mut self, x: i32, y: i32, side: i32) {
        self.disc = (x, y);
        self.disc_px = side.max(1);
    }

    /// Screen-pixel rects of marks that are showing. A width of 0 is none.
    /// The shader still masks the interior so the tint is not measured. The
    /// copy stamps heat from the rects that dispatch used. These are not the
    /// disc size: the seat window still comes from `mask_a.z`.
    pub fn set_marks(&mut self, marks: [[f32; 4]; 3]) {
        self.marks = marks;
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
        stamp_visible_marks(
            &mut heat,
            written.cols,
            written.rows,
            written.cell_px,
            &written.marks,
            &[(written.mask_a, written.disc_a), (written.mask_b, written.disc_b)],
        );
        self.ready = Some(HeatGrid {
            cols: written.cols,
            rows: written.rows,
            cell_px: written.cell_px,
            screen_w: written.screen_w,
            screen_h: written.screen_h,
            heat,
            mask_a: written.mask_a,
            mask_b: written.mask_b,
            disc_a: written.disc_a,
            disc_b: written.disc_b,
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
        let side_a = self.disc_px.max(1);
        let side_b = if self.have_prev { self.prev_disc_px.max(1) } else { side_a };
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
            mask_a: [mask_a.0 as f32, mask_a.1 as f32, side_a as f32, side_a as f32],
            mask_b: [mask_b.0 as f32, mask_b.1 as f32, side_b as f32, side_b as f32],
        };
        unsafe {
            for (i, rect) in self.marks.iter().enumerate() {
                let base = (MARK0 as usize + i) * 4;
                *self.ptr.add(base) = rect[0];
                *self.ptr.add(base + 1) = rect[1];
                *self.ptr.add(base + 2) = rect[2];
                *self.ptr.add(base + 3) = rect[3];
            }
            let host_barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::HOST_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .buffer(self.buf)
                .offset(0)
                .size(vk::WHOLE_SIZE);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::HOST,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[host_barrier],
                &[],
            );
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
        self.prev_disc_px = side_a;
        self.have_prev = true;
        self.written = Some(Written {
            cols,
            rows,
            cell_px: cell,
            screen_w: view.width,
            screen_h: view.height,
            mask_a,
            mask_b,
            disc_a: side_a,
            disc_b: side_b,
            marks: self.marks,
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

/// Distance from a point to a rect. 0 when the point is inside. A width of
/// 1 or less is no rect, matching the shader.
fn rect_outset(px: f32, py: f32, r: [f32; 4]) -> f32 {
    let (x, y, w, h) = (r[0], r[1], r[2], r[3]);
    if w <= 1.0 || h <= 0.0 {
        return f32::MAX;
    }
    let dx = if px < x {
        x - px
    } else if px >= x + w {
        px - (x + w)
    } else {
        0.0
    };
    let dy = if py < y {
        y - py
    } else if py >= y + h {
        py - (y + h)
    } else {
        0.0
    };
    if dx == 0.0 && dy == 0.0 {
        0.0
    } else {
        (dx * dx + dy * dy).sqrt()
    }
}

fn disc_covers(px: f32, py: f32, origin: (i32, i32), side: i32) -> bool {
    side > 1
        && px >= origin.0 as f32
        && py >= origin.1 as f32
        && px < origin.0 as f32 + side as f32
        && py < origin.1 as f32 + side as f32
}

/// While a mark is showing: interior becomes `W_MARK_HOT`, the 12 px margin
/// gains text weight, and the next 40 px cools by `W_MARK_COOL`. Cells under
/// a disc stay as the shader left them. No live rect leaves the grid alone,
/// which is the revert when the mark has stopped showing.
fn stamp_visible_marks(
    heat: &mut [f32],
    cols: u32,
    rows: u32,
    cell_px: u32,
    marks: &[[f32; 4]; 3],
    discs: &[((i32, i32), i32)],
) {
    if cols == 0 || rows == 0 {
        return;
    }
    let n = (cols as usize).saturating_mul(rows as usize);
    if heat.len() != n {
        return;
    }
    let live = marks.iter().any(|r| r[2] > 1.0 && r[3] > 0.0);
    if !live {
        return;
    }
    let cell = cell_px.max(1) as f32;
    let frame_reach = MARK_MARGIN_PX + MARK_FRAME_PX;
    for row in 0..rows {
        for col in 0..cols {
            let cx = (col as f32 + 0.5) * cell;
            let cy = (row as f32 + 0.5) * cell;
            if discs.iter().any(|&(origin, side)| disc_covers(cx, cy, origin, side)) {
                continue;
            }
            let mut inside = false;
            let mut margin = false;
            let mut frame = false;
            for r in marks {
                let d = rect_outset(cx, cy, *r);
                if d <= 0.0 {
                    inside = true;
                    break;
                } else if d <= MARK_MARGIN_PX {
                    margin = true;
                } else if d <= frame_reach {
                    frame = true;
                }
            }
            let i = (row * cols + col) as usize;
            if inside {
                heat[i] = W_MARK_HOT;
            } else if margin {
                heat[i] += W_TEXT;
            } else if frame {
                heat[i] -= W_MARK_COOL;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(fill: f32) -> Vec<f32> {
        vec![fill; 4 * 4]
    }

    #[test]
    fn a_showing_mark_heats_the_inside_and_cools_the_frame() {
        let mut heat = grid(0.5);
        let mut marks = [[0.0; 4]; 3];
        // Cells are 40 px, centers at 20, 60, 100, 140.
        // The rect covers (20, 20) only. (60, 20) is 30 px outside: the frame.
        // (100, 20) is 70 px outside the 52 px reach.
        marks[0] = [0.0, 0.0, 30.0, 30.0];
        stamp_visible_marks(&mut heat, 4, 4, 40, &marks, &[]);
        let at = |c, r| heat[(r * 4 + c) as usize];
        assert_eq!(at(0, 0), W_MARK_HOT);
        assert!((at(1, 0) - (0.5 - W_MARK_COOL)).abs() < 1e-5, "frame {}", at(1, 0));
        assert!((at(2, 0) - 0.5).abs() < 1e-5, "untouched {}", at(2, 0));
    }

    #[test]
    fn the_margin_adds_text_weight_and_a_gone_mark_adds_nothing() {
        let mut heat = grid(0.2);
        let mut marks = [[0.0; 4]; 3];
        // Center (60, 60) is 4 px outside a rect that ends at x=56.
        marks[0] = [0.0, 48.0, 56.0, 24.0];
        stamp_visible_marks(&mut heat, 4, 4, 40, &marks, &[]);
        let margin = heat[1 * 4 + 1];
        assert!((margin - (0.2 + W_TEXT)).abs() < 1e-5, "margin {margin}");
        let mut cleared = grid(0.2);
        stamp_visible_marks(&mut cleared, 4, 4, 40, &[[0.0; 4]; 3], &[]);
        assert!(cleared.iter().all(|h| (*h - 0.2).abs() < 1e-5));
    }

    #[test]
    fn the_disc_is_not_stamped() {
        let mut heat = grid(0.0);
        let mut marks = [[0.0; 4]; 3];
        marks[0] = [0.0, 0.0, 200.0, 200.0];
        stamp_visible_marks(&mut heat, 4, 4, 40, &marks, &[((0, 0), 80)]);
        assert_eq!(heat[0], 0.0);
        assert_eq!(heat[2 * 4 + 2], W_MARK_HOT);
    }
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
