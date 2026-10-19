//! GPU skin and reconstruct. One dispatch writes `AnimatedSplatGpu` into the
//! existing dynamic-splat buffer. `AvatarActor::evaluate` stays the oracle.
//! The skin-matrix buffer is the length of the loaded GLB skin, not the
//! 64-slot projection palette. The live tick uses this only after `check`
//! matches that oracle on this device.

use crate::actors::gpu_layout::{FLAG_ACTIVE, FLAG_SOFT_GAUSSIAN_FALLOFF};
use crate::avatar::math::Mat4;
use crate::avatar::AvatarActor;
use crate::pipeline::SplatPipeline;
use crate::vulkan::VulkanContext;
use ash::vk;
use std::ffi::c_void;

const SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/avatar_expand.comp.spv"));
const LOCAL: u32 = 256;
const POS_TOL: f32 = 1e-3;
const ROT_DOT: f32 = 0.999;
const FLAGS: u32 = FLAG_ACTIVE | FLAG_SOFT_GAUSSIAN_FALLOFF;

#[repr(C)]
#[derive(Clone, Copy)]
struct ExpandPush {
    slot_base: u32,
    splat_count: u32,
    vert_count: u32,
    tri_count: u32,
    joint_count: u32,
    morph_count: u32,
    owner_id: u32,
    frame: u32,
    flags: u32,
    density: u32,
}

const _: () = assert!(std::mem::size_of::<ExpandPush>() == 40);
const _: () = assert!(std::mem::offset_of!(ExpandPush, flags) == 32);
const _: () = assert!(std::mem::offset_of!(ExpandPush, density) == 36);

#[repr(C)]
#[derive(Clone, Copy)]
struct GpuRec {
    tri: u32,
    b1: f32,
    b2: f32,
    off0: f32,
    off1: f32,
    off2: f32,
    pad0: f32,
    pad1: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GpuFrame {
    e1: [f32; 4],
    e2: [f32; 4],
    n: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<GpuRec>() == 32);
const _: () = assert!(std::mem::size_of::<GpuFrame>() == 48);

struct GpuBuf {
    device: ash::Device,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
    ptr: *mut c_void,
}

impl GpuBuf {
    fn new(vk_ctx: &VulkanContext, size: vk::DeviceSize) -> Result<Self, String> {
        let device = &vk_ctx.device;
        let info = vk::BufferCreateInfo::default()
            .size(size.max(4))
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { device.create_buffer(&info, None) }
            .map_err(|e| format!("expand buffer: {e:?}"))?;
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let mem_props = unsafe {
            vk_ctx.instance.get_physical_device_memory_properties(vk_ctx.physical_device)
        };
        let coherent = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let cached = coherent | vk::MemoryPropertyFlags::HOST_CACHED;
        let pick = |wanted: vk::MemoryPropertyFlags| {
            (0..mem_props.memory_type_count).find(|&i| {
                let ok = requirements.memory_type_bits & (1 << i) != 0;
                let props = mem_props.memory_types[i as usize].property_flags;
                ok && props.contains(wanted)
            })
        };
        let memory_type_index = pick(cached).or_else(|| pick(coherent)).ok_or_else(|| {
            "no HOST_VISIBLE|HOST_COHERENT memory for the expand buffers".to_string()
        })?;
        let alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe { device.allocate_memory(&alloc, None) }
            .map_err(|e| format!("expand memory: {e:?}"))?;
        unsafe { device.bind_buffer_memory(buffer, memory, 0) }
            .map_err(|e| format!("expand bind: {e:?}"))?;
        let ptr = unsafe { device.map_memory(memory, 0, size.max(4), vk::MemoryMapFlags::empty()) }
            .map_err(|e| format!("expand map: {e:?}"))?;
        Ok(Self { device: device.clone(), buffer, memory, size: size.max(4), ptr })
    }

    fn write<T: Copy>(&self, data: &[T]) {
        let byte_len = std::mem::size_of_val(data);
        assert!(
            byte_len as vk::DeviceSize <= self.size,
            "expand write of {byte_len} bytes exceeds {}",
            self.size
        );
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr() as *const u8, self.ptr as *mut u8, byte_len);
        }
    }

}

impl Drop for GpuBuf {
    fn drop(&mut self) {
        unsafe {
            self.device.unmap_memory(self.memory);
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
        }
    }
}

pub struct AvatarExpand {
    device: ash::Device,
    cmd_pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    pipe_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    desc_pool: vk::DescriptorPool,
    set_layout: vk::DescriptorSetLayout,
    set: vk::DescriptorSet,
    rest: GpuBuf,
    joints: GpuBuf,
    weights: GpuBuf,
    indices: GpuBuf,
    records: GpuBuf,
    frames: GpuBuf,
    colors: GpuBuf,
    morphs: GpuBuf,
    morph_w: GpuBuf,
    skin: GpuBuf,
    vert_count: u32,
    tri_count: u32,
    joint_count: u32,
    morph_count: u32,
    splat_count: u32,
    live: bool,
}

impl AvatarExpand {
    pub fn new(vk_ctx: &VulkanContext, pipeline: &SplatPipeline, avatar: &AvatarActor) -> Result<Self, String> {
        let vert_count = avatar.mesh.rest.len();
        let tri_count = avatar.mesh.indices.len() / 3;
        if avatar.mesh.indices.len() != tri_count * 3 {
            return Err("index count is not a multiple of 3".into());
        }
        if avatar.recon_frames().len() != tri_count || avatar.recon_colors().len() != tri_count {
            return Err("rest frames and colors do not match the triangle count".into());
        }
        let joint_count = avatar.rig.names.len();
        if joint_count == 0 {
            return Err("skin has no joints".into());
        }
        let morph_count = avatar.mesh.morph_deltas.len();
        for d in &avatar.mesh.morph_deltas {
            if d.len() != vert_count {
                return Err(format!("morph delta length {} != {vert_count} vertices", d.len()));
            }
        }
        let splat_count = avatar.binding.records.len();
        if splat_count == 0 {
            return Err("splatbind has no records".into());
        }
        if avatar.binding.records.iter().any(|r| r.tri as usize >= tri_count) {
            return Err("splatbind triangle index is past the mesh".into());
        }

        let rest: Vec<[f32; 4]> = avatar.mesh.rest.iter().map(|p| [p[0], p[1], p[2], 0.0]).collect();
        let joints: Vec<[u32; 4]> = avatar
            .mesh
            .joints
            .iter()
            .map(|j| [j[0] as u32, j[1] as u32, j[2] as u32, j[3] as u32])
            .collect();
        let recs: Vec<GpuRec> = avatar
            .binding
            .records
            .iter()
            .map(|r| GpuRec {
                tri: r.tri,
                b1: r.b1,
                b2: r.b2,
                off0: r.off[0],
                off1: r.off[1],
                off2: r.off[2],
                pad0: 0.0,
                pad1: 0.0,
            })
            .collect();
        let frames: Vec<GpuFrame> = avatar
            .recon_frames()
            .iter()
            .map(|f| GpuFrame {
                e1: [f[0][0], f[0][1], f[0][2], 0.0],
                e2: [f[1][0], f[1][1], f[1][2], 0.0],
                n: [f[2][0], f[2][1], f[2][2], 0.0],
            })
            .collect();
        let mut morph_pack = Vec::new();
        for d in &avatar.mesh.morph_deltas {
            for p in d {
                morph_pack.push([p[0], p[1], p[2], 0.0]);
            }
        }
        if morph_pack.is_empty() {
            morph_pack.push([0.0; 4]);
        }

        let rest_b = GpuBuf::new(vk_ctx, (rest.len() * 16) as vk::DeviceSize)?;
        rest_b.write(&rest);
        let joints_b = GpuBuf::new(vk_ctx, (joints.len() * 16) as vk::DeviceSize)?;
        joints_b.write(&joints);
        let weights_b = GpuBuf::new(vk_ctx, (avatar.mesh.weights.len() * 16) as vk::DeviceSize)?;
        weights_b.write(&avatar.mesh.weights);
        let indices_b = GpuBuf::new(vk_ctx, (avatar.mesh.indices.len() * 4) as vk::DeviceSize)?;
        indices_b.write(&avatar.mesh.indices);
        let records_b = GpuBuf::new(vk_ctx, (recs.len() * std::mem::size_of::<GpuRec>()) as vk::DeviceSize)?;
        records_b.write(&recs);
        let frames_b = GpuBuf::new(vk_ctx, (frames.len() * std::mem::size_of::<GpuFrame>()) as vk::DeviceSize)?;
        frames_b.write(&frames);
        let colors_b = GpuBuf::new(vk_ctx, (avatar.recon_colors().len() * 16) as vk::DeviceSize)?;
        colors_b.write(avatar.recon_colors());
        let morphs_b = GpuBuf::new(vk_ctx, (morph_pack.len() * 16) as vk::DeviceSize)?;
        morphs_b.write(&morph_pack);
        let morph_w_b = GpuBuf::new(vk_ctx, (morph_count.max(1) * 4) as vk::DeviceSize)?;
        morph_w_b.write(&vec![0.0f32; morph_count.max(1)]);
        let skin_b = GpuBuf::new(vk_ctx, (joint_count * std::mem::size_of::<Mat4>()) as vk::DeviceSize)?;
        skin_b.write(&vec![crate::avatar::math::MAT4_IDENTITY; joint_count]);

        let (splat_buf, splat_size) = pipeline.dynamic_splats_buffer();
        let device = &vk_ctx.device;
        let bindings: Vec<vk::DescriptorSetLayoutBinding> = (0..11)
            .map(|binding| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(binding)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();
        let dsl_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let set_layout = unsafe { device.create_descriptor_set_layout(&dsl_info, None) }
            .map_err(|e| format!("expand set layout: {e:?}"))?;
        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 11,
        }];
        let pool_info = vk::DescriptorPoolCreateInfo::default().pool_sizes(&pool_sizes).max_sets(1);
        let desc_pool = unsafe { device.create_descriptor_pool(&pool_info, None) }
            .map_err(|e| format!("expand descriptor pool: {e:?}"))?;
        let layouts = [set_layout];
        let alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(desc_pool)
            .set_layouts(&layouts);
        let set = unsafe { device.allocate_descriptor_sets(&alloc) }
            .map_err(|e| format!("expand descriptor set: {e:?}"))?[0];

        let bufs = [
            &rest_b,
            &joints_b,
            &weights_b,
            &indices_b,
            &records_b,
            &frames_b,
            &colors_b,
            &morphs_b,
            &morph_w_b,
            &skin_b,
        ];
        for (i, buf) in bufs.iter().enumerate() {
            write_storage(device, set, i as u32, buf.buffer, buf.size);
        }
        write_storage(device, set, 10, splat_buf, splat_size);

        let push = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(std::mem::size_of::<ExpandPush>() as u32);
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(std::slice::from_ref(&push));
        let pipe_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
            .map_err(|e| format!("expand pipeline layout: {e:?}"))?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(SPV))
            .map_err(|e| format!("expand spirv: {e}"))?;
        let module = unsafe {
            device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)
        }
        .map_err(|e| format!("expand shader module: {e:?}"))?;
        let entry = c"main";
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(module)
            .name(entry);
        let create = vk::ComputePipelineCreateInfo::default().stage(stage).layout(pipe_layout);
        let comp = unsafe { device.create_compute_pipelines(vk::PipelineCache::null(), &[create], None) }
            .map_err(|(_, e)| format!("expand pipeline: {e:?}"))?[0];
        unsafe { device.destroy_shader_module(module, None) };

        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(vk_ctx.graphics_queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let cmd_pool = unsafe { device.create_command_pool(&pool_info, None) }
            .map_err(|e| format!("expand command pool: {e:?}"))?;
        let cmd = unsafe {
            device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(cmd_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        }
        .map_err(|e| format!("expand command buffer: {e:?}"))?[0];

        println!(
            "[avatar] gpu expand ready: {splat_count} splats, {vert_count} verts, {tri_count} tris, {joint_count} joints, {morph_count} morphs"
        );
        Ok(Self {
            device: vk_ctx.device.clone(),
            cmd_pool,
            cmd,
            pipe_layout,
            pipeline: comp,
            desc_pool,
            set_layout,
            set,
            rest: rest_b,
            joints: joints_b,
            weights: weights_b,
            indices: indices_b,
            records: records_b,
            frames: frames_b,
            colors: colors_b,
            morphs: morphs_b,
            morph_w: morph_w_b,
            skin: skin_b,
            vert_count: vert_count as u32,
            tri_count: tri_count as u32,
            joint_count: joint_count as u32,
            morph_count: morph_count as u32,
            splat_count: splat_count as u32,
            live: false,
        })
    }

    pub fn is_live(&self) -> bool {
        self.live
    }

    pub fn set_live(&mut self, live: bool) {
        self.live = live;
    }

    pub fn command_buffer(&self) -> vk::CommandBuffer {
        self.cmd
    }

    /// Breath, a side point, and an open jaw. Compares every splat to
    /// `evaluate`, then projects that cloud. The pose clocks are cleared
    /// before this returns.
    pub fn check(&mut self, avatar: &mut AvatarActor, pipeline: &SplatPipeline) -> Result<String, String> {
        let result = self.check_poses(avatar, pipeline);
        avatar.clear_check_pose();
        result
    }

    fn check_poses(&mut self, avatar: &mut AvatarActor, pipeline: &SplatPipeline) -> Result<String, String> {
        let side = crate::placement::DISC_PX as f32;
        let base = pipeline.avatar_slot_base();
        let n = (self.splat_count).min(pipeline.capacity().saturating_sub(base));
        pipeline.set_projection_uniforms(AvatarActor::layer_camera(side, side, 1));
        let poses = [
            ("breath", 1.7_f32, 0.0_f32, 0.0_f32, 0.0_f32),
            ("point", 0.3, std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2, 0.0),
            ("jaw", 0.4, 0.0, 0.0, 1.0),
        ];
        let mut reports = Vec::new();
        let mut failed = false;
        for (name, time, aim, yaw, jaw) in poses {
            avatar.pose_for_check(time, aim, yaw, jaw);
            avatar.evaluate();
            let count = avatar.splat_count() as u32;
            pipeline.clear_projected(base, count);
            if let Err(e) = self.record(avatar.skin_matrices(), avatar.morph_weights(), base, n, 1, 1) {
                failed = true;
                reports.push(format!("{name} record failed: {e}"));
                continue;
            }
            if let Err(e) = pipeline.tick_with(1, 600, Some(self.cmd)) {
                failed = true;
                reports.push(format!("{name} submit failed: {e}"));
                continue;
            }
            let gpu = pipeline.read_splats(base, n);
            let (line, ok) = compare_pose(name, &avatar.posed, &gpu, pipeline.count_projected_discs(base, n));
            if !ok {
                failed = true;
            }
            reports.push(line);
        }
        let report = reports.join("; ");
        if failed {
            Err(report)
        } else {
            Ok(report)
        }
    }

    /// Host-write the palette and record the dispatch. The caller submits
    /// `command_buffer` ahead of projection, in the same queue submit.
    pub fn record(
        &mut self,
        mats: &[Mat4],
        morphs: &[f32],
        slot_base: u32,
        splat_count: u32,
        owner_id: u32,
        frame: u32,
    ) -> Result<(), String> {
        if mats.len() != self.joint_count as usize {
            return Err(format!(
                "skin matrices {} != {} joints",
                mats.len(),
                self.joint_count
            ));
        }
        if splat_count == 0 || splat_count > self.splat_count {
            return Err(format!("splat count {splat_count} outside 1..={}", self.splat_count));
        }
        self.skin.write(mats);
        let mut weights = vec![0.0f32; self.morph_count.max(1) as usize];
        for (i, w) in morphs.iter().take(self.morph_count as usize).enumerate() {
            weights[i] = *w;
        }
        self.morph_w.write(&weights);

        let push = ExpandPush {
            slot_base,
            splat_count,
            vert_count: self.vert_count,
            tri_count: self.tri_count,
            joint_count: self.joint_count,
            morph_count: self.morph_count,
            owner_id,
            frame,
            flags: FLAGS,
            density: 255,
        };
        unsafe {
            self.device
                .reset_command_buffer(self.cmd, vk::CommandBufferResetFlags::empty())
                .map_err(|e| format!("expand reset: {e:?}"))?;
            self.device
                .begin_command_buffer(self.cmd, &vk::CommandBufferBeginInfo::default())
                .map_err(|e| format!("expand begin: {e:?}"))?;
            let host = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::HOST_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);
            self.device.cmd_pipeline_barrier(
                self.cmd,
                vk::PipelineStageFlags::HOST,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[host],
                &[],
                &[],
            );
            self.device.cmd_bind_pipeline(self.cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            self.device.cmd_bind_descriptor_sets(
                self.cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.pipe_layout,
                0,
                &[self.set],
                &[],
            );
            self.device.cmd_push_constants(
                self.cmd,
                self.pipe_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                pod_bytes(&push),
            );
            self.device.cmd_dispatch(self.cmd, splat_count.div_ceil(LOCAL), 1, 1);
            // Publish the cloud to the projection dispatch later in this submit,
            // and to a host read after the fence.
            let release = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(
                    vk::AccessFlags::SHADER_READ
                        | vk::AccessFlags::SHADER_WRITE
                        | vk::AccessFlags::HOST_READ
                        | vk::AccessFlags::UNIFORM_READ,
                );
            self.device.cmd_pipeline_barrier(
                self.cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[release],
                &[],
                &[],
            );
            self.device
                .end_command_buffer(self.cmd)
                .map_err(|e| format!("expand end: {e:?}"))?;
        }
        Ok(())
    }
}

impl Drop for AvatarExpand {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_command_pool(self.cmd_pool, None);
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.pipe_layout, None);
            self.device.destroy_descriptor_pool(self.desc_pool, None);
            self.device.destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}

fn write_storage(device: &ash::Device, set: vk::DescriptorSet, binding: u32, buffer: vk::Buffer, range: vk::DeviceSize) {
    let info = [vk::DescriptorBufferInfo { buffer, offset: 0, range }];
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .buffer_info(&info);
    unsafe { device.update_descriptor_sets(&[write], &[]) };
}

fn pod_bytes<T>(v: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }
}

fn compare_pose(
    name: &str,
    cpu: &[crate::actors::scene_memory::GaussianSplat],
    gpu: &[crate::actors::gpu_layout::AnimatedSplatGpu],
    discs: u32,
) -> (String, bool) {
    let n = cpu.len().min(gpu.len());
    let mut max_pos = 0.0f32;
    let mut min_dot = 1.0f32;
    let mut worst = 0usize;
    let mut bad_flags = 0u32;
    let mut bad_color = 0u32;
    let mut bad_rigid = 0u32;
    for i in 0..n {
        let c = &cpu[i];
        let g = &gpu[i];
        let dp = (0..3)
            .map(|k| (c.position[k] - g.position_and_confidence[k]).abs())
            .fold(0.0f32, f32::max);
        let dot = (0..4).map(|k| c.rotation[k] * g.rotation[k]).sum::<f32>().abs();
        if dp > max_pos {
            max_pos = dp;
            worst = i;
        }
        if dot < min_dot {
            min_dot = dot;
            if dp == max_pos {
                worst = i;
            }
        }
        if g.flags != FLAGS || g.padding != 255 || (g.position_and_confidence[3] - 1.0).abs() > 1e-5 {
            bad_flags += 1;
        }
        if (0..4).any(|k| (c.color[k] - g.color[k]).abs() > 1e-5) {
            bad_color += 1;
        }
        if g.joint_ids != [0, 0, 0, 0] || (g.weights[0] - 1.0).abs() > 1e-5 {
            bad_rigid += 1;
        }
    }
    let ok = n > 0
        && max_pos <= POS_TOL
        && min_dot >= ROT_DOT
        && bad_flags == 0
        && bad_color == 0
        && bad_rigid == 0
        && discs >= 1000;
    let line = format!(
        "{name} n={n} |dp|={max_pos:.3e} |dot|={min_dot:.6} discs={discs} flags={bad_flags} color={bad_color} rigid={bad_rigid} worst={worst}"
    );
    (line, ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_push_and_records_match_std430() {
        assert_eq!(std::mem::size_of::<ExpandPush>(), 40);
        assert_eq!(std::mem::size_of::<GpuRec>(), 32);
        assert_eq!(std::mem::size_of::<GpuFrame>(), 48);
    }
}
