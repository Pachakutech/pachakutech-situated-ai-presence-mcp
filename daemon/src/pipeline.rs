//! The actual GPU compute dispatch loop: allocates the buffers
//! `daemon/shaders/*.comp` expect, builds the two compute pipelines
//! (projection, eviction), and runs them. This is the layer under
//! `docs/gpu-splat-pipeline.md` that turns "the shaders compile" into
//! "the shaders run and produce numbers" — verified in this sandbox
//! against Mesa's `llvmpipe` software Vulkan driver (no real GPU needed
//! to prove the pipeline logic is correct; only to make it fast).
//!
//! Scope, stated plainly: one queue, one command buffer, submitted and
//! waited on synchronously every tick (`vkQueueSubmit` + fence wait, no
//! double-buffering/overlap with the next frame's host writes). That's
//! the right first version — correctness before pipelining — and the
//! thing to revisit once this is a bottleneck, not before. All buffers
//! are `HOST_VISIBLE | HOST_COHERENT` and persistently mapped, so the CPU
//! (ingress actors, the MCP-driven Presence actor) can write splat data
//! directly with no staging buffer — again, simplicity first; a
//! discrete-GPU-optimized version would want device-local memory with
//! staging transfers instead.

use crate::actors::gpu_layout::{
    AnimatedSplatGpu, DualQuatGpu, EvictionUniformsGpu, ProjectedSplatGpu, ProjectionUniformsGpu,
    FLAG_ACTIVE, IDENTITY_DUAL_QUAT, MAX_BONES, MAX_EVICTIONS_PER_FRAME,
};
use crate::vulkan::VulkanContext;
use ash::vk;
use std::ffi::c_void;

const PROJECTION_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/splat_projection.comp.spv"));
const EVICTION_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/splat_eviction.comp.spv"));

/// One GPU allocation + the buffer bound to it, permanently host-mapped.
/// Every SSBO/UBO this pipeline owns is one of these — see the module doc
/// comment for why "always host-visible, always mapped" is the right
/// starting tradeoff here.
struct MappedBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
    mapped_ptr: *mut c_void,
}

impl MappedBuffer {
    fn new(vk_ctx: &VulkanContext, size: vk::DeviceSize, usage: vk::BufferUsageFlags) -> Result<Self, String> {
        let device = &vk_ctx.device;
        let create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { device.create_buffer(&create_info, None) }
            .map_err(|e| format!("vkCreateBuffer failed: {e:?}"))?;

        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let mem_props = unsafe { vk_ctx.instance.get_physical_device_memory_properties(vk_ctx.physical_device) };
        let wanted = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let memory_type_index = (0..mem_props.memory_type_count)
            .find(|&i| {
                let type_supported = requirements.memory_type_bits & (1 << i) != 0;
                let props_supported = mem_props.memory_types[i as usize].property_flags.contains(wanted);
                type_supported && props_supported
            })
            .ok_or_else(|| "no HOST_VISIBLE|HOST_COHERENT memory type supports this buffer".to_string())?;

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe { device.allocate_memory(&alloc_info, None) }.map_err(|e| {
            format!("vkAllocateMemory failed ({} bytes): {e:?}", requirements.size)
        })?;

        unsafe { device.bind_buffer_memory(buffer, memory, 0) }
            .map_err(|e| format!("vkBindBufferMemory failed: {e:?}"))?;

        let mapped_ptr = unsafe { device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty()) }
            .map_err(|e| format!("vkMapMemory failed: {e:?}"))?;

        Ok(Self { buffer, memory, size, mapped_ptr })
    }

    /// Writes `data` starting at byte 0 of the mapped region. Panics if
    /// `data` doesn't fit — a programmer error (wrong capacity passed
    /// somewhere), not a runtime condition to recover from.
    fn write<T: Copy>(&self, data: &[T]) {
        let byte_len = std::mem::size_of_val(data);
        assert!(
            byte_len as vk::DeviceSize <= self.size,
            "write of {byte_len} bytes doesn't fit this {}-byte buffer",
            self.size
        );
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr() as *const u8, self.mapped_ptr as *mut u8, byte_len);
        }
    }

    /// Reads `count` elements of `T` back out of the mapped region. Used
    /// for the projected-output and evicted-textures buffers — genuinely
    /// reading GPU-written results back on the CPU, not a placeholder.
    fn read<T: Copy>(&self, count: usize) -> Vec<T> {
        let mut out = Vec::with_capacity(count);
        unsafe {
            std::ptr::copy_nonoverlapping(self.mapped_ptr as *const T, out.as_mut_ptr(), count);
            out.set_len(count);
        }
        out
    }

    unsafe fn destroy(&self, device: &ash::Device) {
        device.unmap_memory(self.memory);
        device.destroy_buffer(self.buffer, None);
        device.free_memory(self.memory, None);
    }
}

fn create_shader_module(device: &ash::Device, spirv_bytes: &[u8]) -> Result<vk::ShaderModule, String> {
    // SPIR-V is a stream of u32 words; the bytes embedded via
    // `include_bytes!` need reinterpreting, not reparsing — `ash`'s
    // loader helper does exactly that (and checks the magic number/
    // alignment for us).
    let code = ash::util::read_spv(&mut std::io::Cursor::new(spirv_bytes))
        .map_err(|e| format!("SPIR-V embedded in the binary is malformed: {e}"))?;
    let create_info = vk::ShaderModuleCreateInfo::default().code(&code);
    unsafe { device.create_shader_module(&create_info, None) }.map_err(|e| format!("vkCreateShaderModule failed: {e:?}"))
}

/// Everything needed to run one compute shader: its descriptor set layout
/// (so we know how to bind buffers to it), pipeline, and the one
/// descriptor set actually bound to this pipeline's real buffers.
struct ComputeStage {
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    descriptor_set: vk::DescriptorSet,
}

pub struct SplatPipeline {
    /// Cloned from `VulkanContext` so `Drop` can tear GPU objects down
    /// without borrowing the context (which must outlive this pipeline).
    device: ash::Device,
    graphics_queue: vk::Queue,
    capacity: u32,
    dynamic_splats: MappedBuffer,
    bone_palette: MappedBuffer,
    projected_output: MappedBuffer,
    projection_uniforms: MappedBuffer,
    eviction_uniforms: MappedBuffer,
    evicted_textures: MappedBuffer,
    projection: ComputeStage,
    eviction: ComputeStage,
    descriptor_pool: vk::DescriptorPool,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
}

/// `ceil(total / group_size)` — the shaders declare
/// `local_size_x = 256`, so a capacity of, say, 64 still needs 1 full
/// workgroup dispatched (the shader's own `if (id >= total_capacity)
/// return;` guard handles the excess lanes doing nothing).
fn workgroup_count(total: u32, group_size: u32) -> u32 {
    total.div_ceil(group_size)
}

impl SplatPipeline {
    /// `capacity` is the max number of live `AnimatedSplat` entries this
    /// pipeline can hold — the same number the shaders' `total_capacity`
    /// uniform gets set to, and the size `DynamicSplatBuffer`/
    /// `ProjectedOutput` are allocated to.
    pub fn new(vk_ctx: &VulkanContext, capacity: u32) -> Result<Self, String> {
        let device = &vk_ctx.device;

        // ---- Buffers ----
        let splat_stride = std::mem::size_of::<AnimatedSplatGpu>() as vk::DeviceSize;
        let dynamic_splats = MappedBuffer::new(
            vk_ctx,
            splat_stride * capacity as vk::DeviceSize,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?;
        // Every slot starts inactive (all-zero: flags=0 means not
        // ACTIVE) until something calls `write_splat`.
        dynamic_splats.write(&vec![0u8; (splat_stride * capacity as vk::DeviceSize) as usize]);

        let bone_palette = MappedBuffer::new(
            vk_ctx,
            std::mem::size_of::<DualQuatGpu>() as vk::DeviceSize * MAX_BONES as vk::DeviceSize,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?;
        // Every bone starts as identity — an unrigged/rigid splat bound
        // to bone 0 with weight 1.0 (see gpu_layout::to_gpu_splat's
        // default) is then simply un-transformed until something poses
        // bone 0 for real.
        bone_palette.write(&vec![IDENTITY_DUAL_QUAT; MAX_BONES as usize]);

        let projected_output = MappedBuffer::new(
            vk_ctx,
            std::mem::size_of::<ProjectedSplatGpu>() as vk::DeviceSize * capacity as vk::DeviceSize,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?;

        let projection_uniforms = MappedBuffer::new(
            vk_ctx,
            std::mem::size_of::<ProjectionUniformsGpu>() as vk::DeviceSize,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
        )?;

        let eviction_uniforms = MappedBuffer::new(
            vk_ctx,
            std::mem::size_of::<EvictionUniformsGpu>() as vk::DeviceSize,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
        )?;

        // `count: u32` (the atomic) followed by `layer_indices: u32[]`.
        let evicted_textures_size = (4 + 4 * MAX_EVICTIONS_PER_FRAME) as vk::DeviceSize;
        let evicted_textures =
            MappedBuffer::new(vk_ctx, evicted_textures_size, vk::BufferUsageFlags::STORAGE_BUFFER)?;
        evicted_textures.write(&[0u32]); // count = 0 to start

        // ---- Projection stage ----
        let projection_bindings = [
            descriptor_binding(0, vk::DescriptorType::UNIFORM_BUFFER),
            descriptor_binding(1, vk::DescriptorType::STORAGE_BUFFER), // BonePalette
            descriptor_binding(2, vk::DescriptorType::STORAGE_BUFFER), // DynamicSplatBuffer
            descriptor_binding(3, vk::DescriptorType::STORAGE_BUFFER), // ProjectedOutput
        ];
        let projection_layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&projection_bindings);
        let projection_set_layout = unsafe { device.create_descriptor_set_layout(&projection_layout_info, None) }
            .map_err(|e| format!("vkCreateDescriptorSetLayout (projection) failed: {e:?}"))?;
        let projection_module = create_shader_module(device, PROJECTION_SPV)?;
        let (projection_pipeline_layout, projection_pipeline) =
            create_compute_pipeline(device, projection_set_layout, projection_module)?;
        unsafe { device.destroy_shader_module(projection_module, None) };

        // ---- Eviction stage ----
        let eviction_bindings = [
            descriptor_binding(0, vk::DescriptorType::UNIFORM_BUFFER),
            descriptor_binding(1, vk::DescriptorType::STORAGE_BUFFER), // DynamicSplatBuffer
            descriptor_binding(2, vk::DescriptorType::STORAGE_BUFFER), // EvictedTextures
        ];
        let eviction_layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&eviction_bindings);
        let eviction_set_layout = unsafe { device.create_descriptor_set_layout(&eviction_layout_info, None) }
            .map_err(|e| format!("vkCreateDescriptorSetLayout (eviction) failed: {e:?}"))?;
        let eviction_module = create_shader_module(device, EVICTION_SPV)?;
        let (eviction_pipeline_layout, eviction_pipeline) =
            create_compute_pipeline(device, eviction_set_layout, eviction_module)?;
        unsafe { device.destroy_shader_module(eviction_module, None) };

        // ---- Descriptor pool + sets ----
        let pool_sizes = [
            vk::DescriptorPoolSize { ty: vk::DescriptorType::UNIFORM_BUFFER, descriptor_count: 2 },
            vk::DescriptorPoolSize { ty: vk::DescriptorType::STORAGE_BUFFER, descriptor_count: 5 },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo::default().pool_sizes(&pool_sizes).max_sets(2);
        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_info, None) }
            .map_err(|e| format!("vkCreateDescriptorPool failed: {e:?}"))?;

        let set_layouts = [projection_set_layout, eviction_set_layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&set_layouts);
        let sets = unsafe { device.allocate_descriptor_sets(&alloc_info) }
            .map_err(|e| format!("vkAllocateDescriptorSets failed: {e:?}"))?;
        let (projection_set, eviction_set) = (sets[0], sets[1]);

        write_buffer_descriptor(device, projection_set, 0, vk::DescriptorType::UNIFORM_BUFFER, projection_uniforms.buffer, projection_uniforms.size);
        write_buffer_descriptor(device, projection_set, 1, vk::DescriptorType::STORAGE_BUFFER, bone_palette.buffer, bone_palette.size);
        write_buffer_descriptor(device, projection_set, 2, vk::DescriptorType::STORAGE_BUFFER, dynamic_splats.buffer, dynamic_splats.size);
        write_buffer_descriptor(device, projection_set, 3, vk::DescriptorType::STORAGE_BUFFER, projected_output.buffer, projected_output.size);

        write_buffer_descriptor(device, eviction_set, 0, vk::DescriptorType::UNIFORM_BUFFER, eviction_uniforms.buffer, eviction_uniforms.size);
        write_buffer_descriptor(device, eviction_set, 1, vk::DescriptorType::STORAGE_BUFFER, dynamic_splats.buffer, dynamic_splats.size);
        write_buffer_descriptor(device, eviction_set, 2, vk::DescriptorType::STORAGE_BUFFER, evicted_textures.buffer, evicted_textures.size);

        // ---- Command buffer, recorded once ----
        let pool_create_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(vk_ctx.graphics_queue_family)
            .flags(vk::CommandPoolCreateFlags::empty());
        let command_pool = unsafe { device.create_command_pool(&pool_create_info, None) }
            .map_err(|e| format!("vkCreateCommandPool failed: {e:?}"))?;

        let cmd_alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = unsafe { device.allocate_command_buffers(&cmd_alloc_info) }
            .map_err(|e| format!("vkAllocateCommandBuffers failed: {e:?}"))?[0];

        record_dispatch_commands(
            device,
            command_buffer,
            &projection_pipeline,
            projection_pipeline_layout,
            projection_set,
            &eviction_pipeline,
            eviction_pipeline_layout,
            eviction_set,
            workgroup_count(capacity, 256),
        )?;

        let fence_info = vk::FenceCreateInfo::default();
        let fence = unsafe { device.create_fence(&fence_info, None) }.map_err(|e| format!("vkCreateFence failed: {e:?}"))?;

        Ok(Self {
            device: vk_ctx.device.clone(),
            graphics_queue: vk_ctx.graphics_queue,
            capacity,
            dynamic_splats,
            bone_palette,
            projected_output,
            projection_uniforms,
            eviction_uniforms,
            evicted_textures,
            projection: ComputeStage {
                descriptor_set_layout: projection_set_layout,
                pipeline_layout: projection_pipeline_layout,
                pipeline: projection_pipeline,
                descriptor_set: projection_set,
            },
            eviction: ComputeStage {
                descriptor_set_layout: eviction_set_layout,
                pipeline_layout: eviction_pipeline_layout,
                pipeline: eviction_pipeline,
                descriptor_set: eviction_set,
            },
            descriptor_pool,
            command_pool,
            command_buffer,
            fence,
        })
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Writes one `AnimatedSplat` into `DynamicSplatBuffer` at `index`.
    /// Out-of-range indices are refused (logged) rather than panicking —
    /// an ingest that overruns capacity should skip, not take down the daemon.
    pub fn write_splat(&self, index: u32, splat: &AnimatedSplatGpu) {
        if index >= self.capacity {
            eprintln!(
                "[pipeline] write_splat index {index} out of range (capacity {}) — skipped",
                self.capacity
            );
            return;
        }
        let stride = std::mem::size_of::<AnimatedSplatGpu>();
        unsafe {
            let dst = (self.dynamic_splats.mapped_ptr as *mut u8).add(index as usize * stride) as *mut AnimatedSplatGpu;
            dst.write_unaligned(*splat);
        }
    }

    /// Poses one bone. Bone 0 defaults to identity (see `SplatPipeline::new`)
    /// — an unrigged splat cloud rigidly bound to bone 0 moves as a whole
    /// body under whatever this sets bone 0 to, including never calling
    /// this at all (stays identity, i.e. static). Indices at or past
    /// `MAX_BONES` are clamped; the shader would otherwise OOB the palette SSBO.
    pub fn write_bone(&self, index: u32, dual_quat: DualQuatGpu) {
        let index = if index >= MAX_BONES {
            eprintln!("[pipeline] write_bone index {index} >= {MAX_BONES} — clamped");
            MAX_BONES - 1
        } else {
            index
        };
        unsafe {
            let dst = (self.bone_palette.mapped_ptr as *mut u8)
                .add(index as usize * std::mem::size_of::<DualQuatGpu>()) as *mut DualQuatGpu;
            dst.write_unaligned(dual_quat);
        }
    }

    pub fn set_projection_uniforms(&self, mut uniforms: ProjectionUniformsGpu) {
        // The dispatch is recorded against `self.capacity`; a caller-supplied
        // total_capacity that disagrees would either skip live slots or walk
        // off the buffer. Always stamp the allocated size.
        uniforms.total_capacity = self.capacity;
        self.projection_uniforms.write(std::slice::from_ref(&uniforms));
    }

    /// Runs one full tick: projection dispatch, then eviction dispatch,
    /// synchronously (submits the command buffer recorded at
    /// construction time and blocks until the fence signals). Resets the
    /// eviction uniforms/evicted-count for this frame first, since
    /// `EvictedTextures.count` is an accumulating atomic the shader adds
    /// to, not something it resets itself.
    pub fn tick(&self, current_frame: u32, absolute_max_age: u32) -> Result<(), String> {
        self.eviction_uniforms.write(std::slice::from_ref(&EvictionUniformsGpu::new(
            self.capacity,
            current_frame,
            absolute_max_age,
        )));
        self.evicted_textures.write(&[0u32]);

        unsafe { self.device.reset_fences(&[self.fence]) }.map_err(|e| format!("vkResetFences failed: {e:?}"))?;

        let command_buffers = [self.command_buffer];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
        unsafe { self.device.queue_submit(self.graphics_queue, &[submit_info], self.fence) }
            .map_err(|e| format!("vkQueueSubmit failed: {e:?}"))?;

        unsafe { self.device.wait_for_fences(&[self.fence], true, u64::MAX) }
            .map_err(|e| format!("vkWaitForFences failed: {e:?}"))?;

        Ok(())
    }

    /// Reads back what the projection shader wrote this tick. Entries for
    /// splats that were inactive or got culled are left at whatever they
    /// were before (the shader returns early without writing them) — for
    /// a freshly created pipeline that means all-zero, which is
    /// indistinguishable from "a real splat with id 0 at the origin".
    /// Callers that care about this distinction should track which
    /// indices they expect to be live themselves (exactly what the
    /// startup smoke test in `main.rs` does).
    pub fn read_projected(&self) -> Vec<ProjectedSplatGpu> {
        self.projected_output.read(self.capacity as usize)
    }

    pub fn evicted_texture_count(&self) -> u32 {
        // The shader atomicAdds then bounds the write; count can exceed the
        // list length. Callers recycling atlas layers must not walk past it.
        self.evicted_textures.read::<u32>(1)[0].min(MAX_EVICTIONS_PER_FRAME)
    }

    /// One-splat identity-camera tick against this live pipeline. Does not
    /// tear it down — slot 0 is deactivated afterwards so ingest can reuse
    /// the buffer from the start. Failure is fatal at daemon startup.
    pub fn smoke(&self) -> Result<String, String> {
        self.set_projection_uniforms(ProjectionUniformsGpu::new(
            IDENTITY_DUAL_QUAT,
            [800.0, 800.0],
            [640.0, 360.0],
            [1280.0, 720.0],
            0.05,
            self.capacity,
            1,
        ));
        self.write_splat(
            0,
            &AnimatedSplatGpu {
                position_and_confidence: [0.0, 0.0, 2.0, 1.0],
                rotation: [1.0, 0.0, 0.0, 0.0],
                color: [1.0, 0.0, 0.0, 1.0],
                joint_ids: [0, 0, 0, 0],
                weights: [1.0, 0.0, 0.0, 0.0],
                owner_id: 1,
                last_visible_frame: 1,
                flags: FLAG_ACTIVE,
                padding: 255,
            },
        );
        self.tick(1, 600)?;
        let projected = self.read_projected();
        let p = projected[0];
        // Identity camera, splat at (0,0,2): screen ≈ principal (640, 360),
        // depth ≈ 2. A zeroed slot means the shader returned early (near
        // clip / cull / thinning) — that's a real pipeline bug, not "empty".
        if p.depth < 1.0 {
            return Err(format!(
                "splat 0 did not project as expected: splat_id={} depth={} screen=({}, {}) radius={}",
                p.splat_id, p.depth, p.screen_center[0], p.screen_center[1], p.radius_pixels
            ));
        }
        // Free the smoke occupant so Control ingest starts at slot 0.
        self.write_splat(0, &INACTIVE_SPLAT);
        Ok(format!(
            "smoke ok: splat 0 -> screen ({:.1}, {:.1}) r={:.2}px depth={:.2} (capacity {}) — pipeline kept live",
            p.screen_center[0], p.screen_center[1], p.radius_pixels, p.depth, self.capacity
        ))
    }
}

pub(crate) const INACTIVE_SPLAT: AnimatedSplatGpu = AnimatedSplatGpu {
    position_and_confidence: [0.0, 0.0, 0.0, 0.0],
    rotation: [1.0, 0.0, 0.0, 0.0],
    color: [0.0, 0.0, 0.0, 0.0],
    joint_ids: [0, 0, 0, 0],
    weights: [0.0, 0.0, 0.0, 0.0],
    owner_id: 0,
    last_visible_frame: 0,
    flags: 0,
    padding: 0,
};

fn descriptor_binding(binding: u32, ty: vk::DescriptorType) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(binding)
        .descriptor_type(ty)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::COMPUTE)
}

fn write_buffer_descriptor(
    device: &ash::Device,
    set: vk::DescriptorSet,
    binding: u32,
    ty: vk::DescriptorType,
    buffer: vk::Buffer,
    range: vk::DeviceSize,
) {
    let buffer_info = [vk::DescriptorBufferInfo { buffer, offset: 0, range }];
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(ty)
        .buffer_info(&buffer_info);
    unsafe { device.update_descriptor_sets(&[write], &[]) };
}

fn create_compute_pipeline(
    device: &ash::Device,
    set_layout: vk::DescriptorSetLayout,
    module: vk::ShaderModule,
) -> Result<(vk::PipelineLayout, vk::Pipeline), String> {
    let set_layouts = [set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .map_err(|e| format!("vkCreatePipelineLayout failed: {e:?}"))?;

    let entry_point = c"main";
    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(module)
        .name(entry_point);

    let create_info = vk::ComputePipelineCreateInfo::default().stage(stage).layout(pipeline_layout);
    let pipelines = unsafe { device.create_compute_pipelines(vk::PipelineCache::null(), &[create_info], None) }
        .map_err(|(_, e)| format!("vkCreateComputePipelines failed: {e:?}"))?;

    Ok((pipeline_layout, pipelines[0]))
}

#[allow(clippy::too_many_arguments)]
fn record_dispatch_commands(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    projection_pipeline: &vk::Pipeline,
    projection_layout: vk::PipelineLayout,
    projection_set: vk::DescriptorSet,
    eviction_pipeline: &vk::Pipeline,
    eviction_layout: vk::PipelineLayout,
    eviction_set: vk::DescriptorSet,
    workgroups: u32,
) -> Result<(), String> {
    let begin_info = vk::CommandBufferBeginInfo::default();
    unsafe { device.begin_command_buffer(cmd, &begin_info) }.map_err(|e| format!("vkBeginCommandBuffer failed: {e:?}"))?;

    unsafe {
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, *projection_pipeline);
        device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::COMPUTE, projection_layout, 0, &[projection_set], &[]);
        device.cmd_dispatch(cmd, workgroups, 1, 1);
    }

    // The eviction shader reads/writes the same DynamicSplatBuffer the
    // projection shader just wrote `last_visible_frame` into — a
    // genuine GPU-GPU hazard within one command buffer that needs an
    // explicit barrier (unlike host<->device synchronization across a
    // queue submission, which the fence wait already covers; see the
    // module doc comment).
    let barrier = vk::MemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
        .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE);
    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[barrier],
            &[],
            &[],
        );
    }

    unsafe {
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, *eviction_pipeline);
        device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::COMPUTE, eviction_layout, 0, &[eviction_set], &[]);
        device.cmd_dispatch(cmd, workgroups, 1, 1);
    }

    unsafe { device.end_command_buffer(cmd) }.map_err(|e| format!("vkEndCommandBuffer failed: {e:?}"))?;
    Ok(())
}

impl Drop for SplatPipeline {
    fn drop(&mut self) {
        // Wait idle so in-flight smoke/ticks finish before we free the
        // command buffer and SSBOs. Locals in `main` drop pipeline before
        // `VulkanContext`, so the device is still alive here.
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_pipeline(self.projection.pipeline, None);
            self.device.destroy_pipeline_layout(self.projection.pipeline_layout, None);
            self.device.destroy_descriptor_set_layout(self.projection.descriptor_set_layout, None);
            self.device.destroy_pipeline(self.eviction.pipeline, None);
            self.device.destroy_pipeline_layout(self.eviction.pipeline_layout, None);
            self.device.destroy_descriptor_set_layout(self.eviction.descriptor_set_layout, None);
            self.dynamic_splats.destroy(&self.device);
            self.bone_palette.destroy(&self.device);
            self.projected_output.destroy(&self.device);
            self.projection_uniforms.destroy(&self.device);
            self.eviction_uniforms.destroy(&self.device);
            self.evicted_textures.destroy(&self.device);
        }
    }
}
