//! Instanced discs for `ProjectedSplat` output. Drawn inside the existing
//! layer-shell render pass. When the avatar range is live this is the
//! whole present: the screen quad is not drawn under it. Isotropic on
//! purpose: the projection compute already collapsed each Gaussian to a radius.

use ash::vk;

const VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/splat_disc.vert.spv"));
const FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/splat_disc.frag.spv"));

#[repr(C)]
struct DiscPush {
    extent: [f32; 2],
    base_slot: u32,
    _pad: u32,
}

pub struct SplatSprites {
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
}

impl SplatSprites {
    pub fn new(
        device: &ash::Device,
        render_pass: vk::RenderPass,
        projected: vk::Buffer,
        projected_bytes: vk::DeviceSize,
    ) -> Result<Self, String> {
        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX);
        let bindings = [binding];
        let dsl_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let set_layout = unsafe { device.create_descriptor_set_layout(&dsl_info, None) }
            .map_err(|e| format!("splat disc set layout: {e:?}"))?;

        let push = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(std::mem::size_of::<DiscPush>() as u32);
        let sets = [set_layout];
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&sets)
            .push_constant_ranges(std::slice::from_ref(&push));
        let layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
            .map_err(|e| format!("splat disc pipeline layout: {e:?}"))?;

        let vert = shader(device, VERT_SPV)?;
        let frag = shader(device, FRAG_SPV)?;
        let stages = [
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vert).name(c"main"),
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(frag).name(c"main"),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
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
        let create = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&msaa)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(layout)
            .render_pass(render_pass)
            .subpass(0);
        let pipelines = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[create], None) }
            .map_err(|(_, e)| format!("splat disc pipeline: {e:?}"))?;
        unsafe {
            device.destroy_shader_module(vert, None);
            device.destroy_shader_module(frag, None);
        }

        let pool_sizes = [vk::DescriptorPoolSize { ty: vk::DescriptorType::STORAGE_BUFFER, descriptor_count: 1 }];
        let pool_info = vk::DescriptorPoolCreateInfo::default().pool_sizes(&pool_sizes).max_sets(1);
        let pool = unsafe { device.create_descriptor_pool(&pool_info, None) }
            .map_err(|e| format!("splat disc pool: {e:?}"))?;
        let layouts = [set_layout];
        let alloc = vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&layouts);
        let set = unsafe { device.allocate_descriptor_sets(&alloc) }
            .map_err(|e| format!("splat disc set: {e:?}"))?[0];
        let info = [vk::DescriptorBufferInfo { buffer: projected, offset: 0, range: projected_bytes }];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&info);
        unsafe { device.update_descriptor_sets(&[write], &[]) };

        Ok(Self { layout, pipeline: pipelines[0], set_layout, pool, set })
    }

    pub fn record(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        extent: vk::Extent2D,
        base_slot: u32,
        count: u32,
    ) {
        if count == 0 {
            return;
        }
        let push = DiscPush {
            extent: [extent.width as f32, extent.height as f32],
            base_slot,
            _pad: 0,
        };
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, self.layout, 0, &[self.set], &[]);
            device.cmd_push_constants(
                cmd,
                self.layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                std::slice::from_raw_parts(&push as *const DiscPush as *const u8, std::mem::size_of::<DiscPush>()),
            );
            device.cmd_draw(cmd, count.saturating_mul(6), 1, 0, 0);
        }
    }

    pub fn destroy(self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}

fn shader(device: &ash::Device, bytes: &[u8]) -> Result<vk::ShaderModule, String> {
    let code = ash::util::read_spv(&mut std::io::Cursor::new(bytes)).map_err(|e| format!("splat disc spirv: {e}"))?;
    let info = vk::ShaderModuleCreateInfo::default().code(&code);
    unsafe { device.create_shader_module(&info, None) }.map_err(|e| format!("splat disc module: {e:?}"))
}
