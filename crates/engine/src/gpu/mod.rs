//! Direct Vulkan 1.3 rendering. Two frames in flight; resources outlive their fences.
mod context;
mod memory;
mod ray_scene;
use crate::assets::*;
use anyhow::Result;
use ash::vk;
use bytemuck::{Pod, Zeroable};
use context::Context;
use memory::{Buffer, Image, image_barrier};
use std::{io::Cursor, sync::Arc};
use winit::window::Window;

const TEXTURES: usize = 64;
const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
const INSTANCE_CAPACITY: usize = 200_000;
pub const STREAM_VERTICES: usize = 11_200_000;
pub const STREAM_INDICES: usize = 24_300_000;
const UPLOAD_BYTES: u64 = 8 * 1024 * 1024;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    pub view_projection: [[f32; 4]; 4],
    pub light_projection: [[f32; 4]; 4],
    pub far_light_projection: [[f32; 4]; 4],
    pub inverse_view_projection: [[f32; 4]; 4],
    pub camera_time: [f32; 4],
    pub sun: [f32; 4],
    pub sky: [f32; 4],
    pub environment: [f32; 4],
    pub weather: [f32; 4],
    pub sun_colour: [f32; 4],
    pub geography_maps: [u32; 4],
}
pub struct Shaders<'a> {
    pub probe_update: &'a [u8],
    pub transport: &'a [u8],
    pub vertex: &'a [u8],
    pub fragment: &'a [u8],
    pub shadow: &'a [u8],
    pub water_fragment: &'a [u8],
    pub rt_fragment: &'a [u8],
    pub rt_water_fragment: &'a [u8],
    pub sky_vertex: &'a [u8],
    pub sky_fragment: &'a [u8],
    pub tonemap_fragment: &'a [u8],
    pub volume_fragment: &'a [u8],
}
pub struct DrawLists<'a> {
    pub opaque: &'a [Draw],
    pub shadows: &'a [Draw],
    pub far_shadows: &'a [Draw],
    pub water: &'a [Draw],
}
pub struct Upload<'a> {
    pub range: MeshRange,
    pub mesh: &'a MeshData,
}
struct Frame {
    command: vk::CommandBuffer,
    fence: vk::Fence,
    acquire: vk::Semaphore,
    uniform: Buffer,
    instances: Buffer,
    staging: Buffer,
    set: vk::DescriptorSet,
}
struct Swapchain {
    api: ash::khr::swapchain::Device,
    handle: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    present: Vec<vk::Semaphore>,
    extent: vk::Extent2D,
    format: vk::Format,
    depth: Image,
    color: Image,
    scene_color: Image,
    hdr: Image,
    volume: Image,
}
pub struct Renderer {
    ctx: Arc<Context>,
    swap: Swapchain,
    pool: vk::CommandPool,
    frames: Vec<Frame>,
    frame: usize,
    vertex: Buffer,
    index: Buffer,
    _materials: Buffer,
    _transport: Buffer,
    _probe_cache: Buffer,
    probe_pipeline: Option<vk::Pipeline>,
    probe_frame: u32,
    timing: Option<vk::QueryPool>,
    timing_ready: [bool; 2],
    timestamp_period: f32,
    _ray_scene: Option<ray_scene::RayScene>,
    _textures: Vec<Image>,
    sampler: vk::Sampler,
    shadow_sampler: vk::Sampler,
    shadow: Image,
    descriptor_pool: vk::DescriptorPool,
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    shadow_pipeline: vk::Pipeline,
    opaque_shadow_pipeline: vk::Pipeline,
    solid_materials: Vec<bool>,
    water_pipeline: vk::Pipeline,
    sky_pipeline: vk::Pipeline,
    tonemap_pipeline: vk::Pipeline,
    volume_pipeline: vk::Pipeline,
    pub stream_vertex_start: usize,
    pub stream_index_start: usize,
    pub device_name: String,
}
impl Renderer {
    pub fn new(window: &Window, scene: &SceneData, shaders: Shaders) -> Result<Self> {
        unsafe {
            anyhow::ensure!(
                !scene.textures.is_empty() && scene.textures.len() <= TEXTURES,
                "Texture count must be 1..64"
            );
            let ctx = Context::new(window)?;
            let props = ctx.instance.get_physical_device_properties(ctx.physical);
            let device_name = std::ffi::CStr::from_ptr(props.device_name.as_ptr())
                .to_string_lossy()
                .into_owned();
            let supported_samples = props.limits.framebuffer_color_sample_counts
                & props.limits.framebuffer_depth_sample_counts;
            anyhow::ensure!(
                supported_samples.contains(vk::SampleCountFlags::TYPE_4),
                "4x MSAA unavailable"
            );
            let pool = ctx.device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(ctx.family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )?;
            let vertex = Buffer::new(
                &ctx,
                ((scene.vertices.len() + STREAM_VERTICES) * std::mem::size_of::<Vertex>()) as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | if ctx.ray_query {
                        vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    } else {
                        vk::BufferUsageFlags::empty()
                    },
                false,
            )?;
            let index = Buffer::new(
                &ctx,
                ((scene.indices.len() + STREAM_INDICES) * 4) as u64,
                vk::BufferUsageFlags::INDEX_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | if ctx.ray_query {
                        vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    } else {
                        vk::BufferUsageFlags::empty()
                    },
                false,
            )?;
            let materials = Buffer::new(
                &ctx,
                (scene.materials.len() * 64) as u64,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                true,
            )?;
            materials.write(0, bytemuck::cast_slice(&scene.materials))?;
            upload_buffer(&ctx, pool, &vertex, bytemuck::cast_slice(&scene.vertices))?;
            upload_buffer(&ctx, pool, &index, bytemuck::cast_slice(&scene.indices))?;
            let ray_scene = if ctx.ray_query {
                Some(ray_scene::RayScene::build(
                    &ctx, pool, scene, &vertex, &index,
                )?)
            } else {
                None
            };
            let bake = scene.transport.unwrap_or(TransportBake {
                occupancy: 0,
                heights: 0,
                size: 1,
                layers: 1,
            });
            let transport = Buffer::new(
                &ctx,
                (bake.size * bake.size * bake.layers * 80) as u64,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                false,
            )?;
            let mut textures = Vec::new();
            for texture in &scene.textures {
                textures.push(upload_texture(&ctx, pool, texture)?);
            }
            let sampler = ctx.device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::REPEAT)
                    .address_mode_v(vk::SamplerAddressMode::REPEAT)
                    .address_mode_w(vk::SamplerAddressMode::REPEAT)
                    .max_lod(16.)
                    .anisotropy_enable(
                        ctx.instance
                            .get_physical_device_features(ctx.physical)
                            .sampler_anisotropy
                            != 0,
                    )
                    .max_anisotropy(16.0_f32.min(props.limits.max_sampler_anisotropy)),
                None,
            )?;
            let shadow_sampler = ctx.device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                    .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE)
                    .compare_enable(true)
                    .compare_op(vk::CompareOp::LESS_OR_EQUAL),
                None,
            )?;
            let shadow = Image::new(
                &ctx,
                4096,
                2048,
                vk::Format::D32_SFLOAT,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                1,
                vk::SampleCountFlags::TYPE_1,
                vk::ImageAspectFlags::DEPTH,
            )?;
            let probe_cache = Buffer::new(
                &ctx,
                6912 * 208,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                false,
            )?;
            immediate(&ctx, pool, |cmd| {
                ctx.device
                    .cmd_fill_buffer(cmd, probe_cache.handle, 0, probe_cache.size, 0);
                let ready = vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE);
                ctx.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COMPUTE_SHADER
                        | vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[ready],
                    &[],
                    &[],
                );
            })?;
            let swap = make_swapchain(&ctx, window, vk::SwapchainKHR::null())?;
            let mut bindings = vec![
                vk::DescriptorSetLayoutBinding::default()
                    .binding(13)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT | vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(12)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(11)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(5)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(6)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::ALL),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(TEXTURES as u32)
                    .stage_flags(
                        vk::ShaderStageFlags::VERTEX
                            | vk::ShaderStageFlags::FRAGMENT
                            | vk::ShaderStageFlags::COMPUTE,
                    ),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::ALL),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(3)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(4)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT | vk::ShaderStageFlags::COMPUTE),
            ];
            if ctx.ray_query {
                bindings.extend([
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(7)
                        .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                        .descriptor_count(1)
                        .stage_flags(
                            vk::ShaderStageFlags::FRAGMENT | vk::ShaderStageFlags::COMPUTE,
                        ),
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(8)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(1)
                        .stage_flags(
                            vk::ShaderStageFlags::FRAGMENT | vk::ShaderStageFlags::COMPUTE,
                        ),
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(9)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(1)
                        .stage_flags(
                            vk::ShaderStageFlags::FRAGMENT | vk::ShaderStageFlags::COMPUTE,
                        ),
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(10)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(1)
                        .stage_flags(
                            vk::ShaderStageFlags::FRAGMENT | vk::ShaderStageFlags::COMPUTE,
                        ),
                ]);
            }
            let set_layout = ctx.device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?;
            let mut sizes = vec![
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: 2,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_BUFFER,
                    descriptor_count: 12,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: (TEXTURES as u32 + 5) * 2,
                },
            ];
            if ctx.ray_query {
                sizes.push(vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                    descriptor_count: 2,
                });
            }
            let descriptor_pool = ctx.device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(2)
                    .pool_sizes(&sizes),
                None,
            )?;
            let sets = ctx.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&[set_layout, set_layout]),
            )?;
            let commands = ctx.device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(2),
            )?;
            let mut frames = Vec::new();
            for i in 0..2 {
                let uniform = Buffer::new(
                    &ctx,
                    std::mem::size_of::<Globals>() as u64,
                    vk::BufferUsageFlags::UNIFORM_BUFFER,
                    true,
                )?;
                let instances = Buffer::new(
                    &ctx,
                    (INSTANCE_CAPACITY * std::mem::size_of::<Instance>()) as u64,
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                    true,
                )?;
                let staging =
                    Buffer::new(&ctx, UPLOAD_BYTES, vk::BufferUsageFlags::TRANSFER_SRC, true)?;
                let u = [vk::DescriptorBufferInfo::default()
                    .buffer(uniform.handle)
                    .range(uniform.size)];
                let m = [vk::DescriptorBufferInfo::default()
                    .buffer(materials.handle)
                    .range(materials.size)];
                let images: Vec<_> = (0..TEXTURES)
                    .map(|n| {
                        vk::DescriptorImageInfo::default()
                            .sampler(sampler)
                            .image_view(textures[n.min(textures.len() - 1)].view)
                            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    })
                    .collect();
                let gi = [vk::DescriptorBufferInfo::default()
                    .buffer(probe_cache.handle)
                    .range(probe_cache.size)];
                let tr = [vk::DescriptorBufferInfo::default()
                    .buffer(transport.handle)
                    .range(transport.size)];
                let hdr = [vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(swap.hdr.view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                let volume = [vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(swap.volume.view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                let sc = [vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(swap.scene_color.view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                let sd = [vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(swap.depth.view)
                    .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
                let sh = [vk::DescriptorImageInfo::default()
                    .sampler(shadow_sampler)
                    .image_view(shadow.view)
                    .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
                ctx.device.update_descriptor_sets(
                    &[
                        vk::WriteDescriptorSet::default()
                            .dst_set(sets[i])
                            .dst_binding(13)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .buffer_info(&gi),
                        vk::WriteDescriptorSet::default()
                            .dst_set(sets[i])
                            .dst_binding(11)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&hdr),
                        vk::WriteDescriptorSet::default()
                            .dst_set(sets[i])
                            .dst_binding(12)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&volume),
                        vk::WriteDescriptorSet::default()
                            .dst_set(sets[i])
                            .dst_binding(5)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&sc),
                        vk::WriteDescriptorSet::default()
                            .dst_set(sets[i])
                            .dst_binding(6)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&sd),
                        vk::WriteDescriptorSet::default()
                            .dst_set(sets[i])
                            .dst_binding(4)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .buffer_info(&tr),
                        vk::WriteDescriptorSet::default()
                            .dst_set(sets[i])
                            .dst_binding(0)
                            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                            .buffer_info(&u),
                        vk::WriteDescriptorSet::default()
                            .dst_set(sets[i])
                            .dst_binding(1)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&images),
                        vk::WriteDescriptorSet::default()
                            .dst_set(sets[i])
                            .dst_binding(2)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .buffer_info(&m),
                        vk::WriteDescriptorSet::default()
                            .dst_set(sets[i])
                            .dst_binding(3)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&sh),
                    ],
                    &[],
                );
                if let Some(ray) = &ray_scene {
                    let handles = [ray.handle()];
                    let mut accels = vk::WriteDescriptorSetAccelerationStructureKHR::default()
                        .acceleration_structures(&handles);
                    let metadata = [vk::DescriptorBufferInfo::default()
                        .buffer(ray.metadata.handle)
                        .range(ray.metadata.size)];
                    let v = [vk::DescriptorBufferInfo::default()
                        .buffer(vertex.handle)
                        .range((scene.vertices.len() * 56) as u64)];
                    let ix = [vk::DescriptorBufferInfo::default()
                        .buffer(index.handle)
                        .range((scene.indices.len() * 4) as u64)];
                    ctx.device.update_descriptor_sets(
                        &[
                            vk::WriteDescriptorSet::default()
                                .dst_set(sets[i])
                                .dst_binding(7)
                                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                                .descriptor_count(1)
                                .push_next(&mut accels),
                            vk::WriteDescriptorSet::default()
                                .dst_set(sets[i])
                                .dst_binding(8)
                                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                                .buffer_info(&metadata),
                            vk::WriteDescriptorSet::default()
                                .dst_set(sets[i])
                                .dst_binding(9)
                                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                                .buffer_info(&v),
                            vk::WriteDescriptorSet::default()
                                .dst_set(sets[i])
                                .dst_binding(10)
                                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                                .buffer_info(&ix),
                        ],
                        &[],
                    );
                }
                frames.push(Frame {
                    command: commands[i],
                    fence: ctx.device.create_fence(
                        &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                        None,
                    )?,
                    acquire: ctx
                        .device
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?,
                    uniform,
                    instances,
                    staging,
                    set: sets[i],
                });
            }
            let push = [vk::PushConstantRange::default()
                .stage_flags(vk::ShaderStageFlags::ALL)
                .size(16)];
            let layout = ctx.device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&[set_layout])
                    .push_constant_ranges(&push),
                None,
            )?;
            bake_transport(&ctx, pool, layout, sets[0], shaders.transport, bake)?;
            let probe_pipeline = if ctx.ray_query && std::env::var("BVL_GI").as_deref() != Ok("0") {
                Some(make_compute_pipeline(&ctx, layout, shaders.probe_update)?)
            } else {
                None
            };
            let pipeline = make_pipeline(
                &ctx,
                layout,
                shaders.vertex,
                if ctx.ray_query {
                    shaders.rt_fragment
                } else {
                    shaders.fragment
                },
                Some(HDR_FORMAT),
                vk::SampleCountFlags::TYPE_4,
                false,
                false,
                true,
            )?;
            let water_pipeline = make_pipeline(
                &ctx,
                layout,
                shaders.vertex,
                if ctx.ray_query {
                    shaders.rt_water_fragment
                } else {
                    shaders.water_fragment
                },
                Some(HDR_FORMAT),
                vk::SampleCountFlags::TYPE_4,
                false,
                false,
                false,
            )?;
            let opaque_shadow_pipeline = make_pipeline(
                &ctx,
                layout,
                shaders.vertex,
                &[], // Solid depth-only pass: no fragment shader or alpha sampling.
                None,
                vk::SampleCountFlags::TYPE_1,
                true,
                false,
                true,
            )?;
            let solid_materials = scene
                .materials
                .iter()
                .map(|m| m.settings[0] == 0. || m.settings[0] == 4.)
                .collect();
            let shadow_pipeline = make_pipeline(
                &ctx,
                layout,
                shaders.vertex,
                shaders.shadow,
                None,
                vk::SampleCountFlags::TYPE_1,
                true,
                false,
                true,
            )?;
            let sky_pipeline = make_pipeline(
                &ctx,
                layout,
                shaders.sky_vertex,
                shaders.sky_fragment,
                Some(HDR_FORMAT),
                vk::SampleCountFlags::TYPE_4,
                false,
                true,
                false,
            )?;
            let tonemap_pipeline = make_pipeline(
                &ctx,
                layout,
                shaders.sky_vertex,
                shaders.tonemap_fragment,
                Some(swap.format),
                vk::SampleCountFlags::TYPE_1,
                false,
                true,
                false,
            )?;
            let volume_pipeline = make_pipeline(
                &ctx,
                layout,
                shaders.sky_vertex,
                shaders.volume_fragment,
                Some(HDR_FORMAT),
                vk::SampleCountFlags::TYPE_1,
                false,
                true,
                false,
            )?;
            println!(
                "Vulkan scene: {} vertices / {} indices / {} textures / {} materials",
                scene.vertices.len(),
                scene.indices.len(),
                textures.len(),
                scene.materials.len()
            );
            let timing = if std::env::var_os("BVL_GPU_PROFILE").is_some() {
                Some(
                    ctx.device.create_query_pool(
                        &vk::QueryPoolCreateInfo::default()
                            .query_type(vk::QueryType::TIMESTAMP)
                            .query_count(12),
                        None,
                    )?,
                )
            } else {
                None
            };
            Ok(Self {
                ctx,
                swap,
                pool,
                frames,
                frame: 0,
                vertex,
                index,
                _materials: materials,
                _transport: transport,
                _probe_cache: probe_cache,
                probe_pipeline,
                probe_frame: 0,
                timing,
                timing_ready: [false; 2],
                timestamp_period: props.limits.timestamp_period,
                _ray_scene: ray_scene,
                _textures: textures,
                sampler,
                shadow_sampler,
                shadow,
                descriptor_pool,
                set_layout,
                layout,
                pipeline,
                shadow_pipeline,
                opaque_shadow_pipeline,
                solid_materials,
                water_pipeline,
                sky_pipeline,
                tonemap_pipeline,
                volume_pipeline,
                stream_vertex_start: scene.vertices.len(),
                stream_index_start: scene.indices.len(),
                device_name,
            })
        }
    }
    pub fn supports_probe_gi(&self) -> bool {
        self.probe_pipeline.is_some()
    }
    pub fn supports_ray_queries(&self) -> bool {
        self.ctx.ray_query
    }
    pub fn aspect(&self) -> f32 {
        self.swap.extent.width as f32 / self.swap.extent.height as f32
    }
    pub fn resize(&mut self, window: &Window) -> Result<()> {
        unsafe {
            self.ctx.device.device_wait_idle()?;
            let next = make_swapchain(&self.ctx, window, self.swap.handle)?;
            destroy_swapchain(&self.ctx, &self.swap);
            self.swap = next;
            for f in &self.frames {
                let volume = [vk::DescriptorImageInfo::default()
                    .sampler(self.sampler)
                    .image_view(self.swap.volume.view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                let hdr = [vk::DescriptorImageInfo::default()
                    .sampler(self.sampler)
                    .image_view(self.swap.hdr.view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                let sc = [vk::DescriptorImageInfo::default()
                    .sampler(self.sampler)
                    .image_view(self.swap.scene_color.view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                let sd = [vk::DescriptorImageInfo::default()
                    .sampler(self.sampler)
                    .image_view(self.swap.depth.view)
                    .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
                self.ctx.device.update_descriptor_sets(
                    &[
                        vk::WriteDescriptorSet::default()
                            .dst_set(f.set)
                            .dst_binding(11)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&hdr),
                        vk::WriteDescriptorSet::default()
                            .dst_set(f.set)
                            .dst_binding(12)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&volume),
                        vk::WriteDescriptorSet::default()
                            .dst_set(f.set)
                            .dst_binding(5)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&sc),
                        vk::WriteDescriptorSet::default()
                            .dst_set(f.set)
                            .dst_binding(6)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&sd),
                    ],
                    &[],
                );
            }
        }
        Ok(())
    }
    pub fn draw(
        &mut self,
        globals: &Globals,
        instances: &[Instance],
        lists: DrawLists,
        upload: Option<Upload>,
        capture: Option<&std::path::Path>,
    ) -> Result<bool> {
        let DrawLists {
            opaque: draws,
            shadows: shadow_draws,
            far_shadows: far_shadow_draws,
            water: water_draws,
        } = lists;
        unsafe {
            let f = &self.frames[self.frame];
            let device = &self.ctx.device;
            device.wait_for_fences(&[f.fence], true, u64::MAX)?;
            if let Some(pool) = self.timing
                && self.timing_ready[self.frame]
            {
                let mut stamps = [0u64; 6];
                device.get_query_pool_results(
                    pool,
                    self.frame as u32 * 6,
                    &mut stamps,
                    vk::QueryResultFlags::TYPE_64,
                )?;
                if self.probe_frame.is_multiple_of(120) {
                    let ms = std::array::from_fn::<_, 5, _>(|i| {
                        (stamps[i + 1] - stamps[i]) as f64 * self.timestamp_period as f64
                            / 1_000_000.
                    });
                    println!(
                        "GPU ms: GI {:.2}, shadows {:.2}, scene {:.2}, volume {:.2}, tone {:.2}",
                        ms[0], ms[1], ms[2], ms[3], ms[4]
                    );
                }
            }
            let acquired = self.swap.api.acquire_next_image(
                self.swap.handle,
                u64::MAX,
                f.acquire,
                vk::Fence::null(),
            );
            let (image, suboptimal) = match acquired {
                Ok(v) => v,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(false),
                Err(e) => return Err(e.into()),
            };
            f.uniform.write(0, bytemuck::bytes_of(globals))?;
            f.instances.write(0, bytemuck::cast_slice(instances))?;
            device.reset_fences(&[f.fence])?;
            device.reset_command_buffer(f.command, vk::CommandBufferResetFlags::empty())?;
            let cmd = f.command;
            device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            if let Some(pool) = self.timing {
                device.cmd_reset_query_pool(cmd, pool, self.frame as u32 * 6, 6);
                device.cmd_write_timestamp(
                    cmd,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    pool,
                    self.frame as u32 * 6,
                );
                self.timing_ready[self.frame] = true;
            }
            if let Some(u) =
                upload.filter(|u| !u.mesh.indices.is_empty() && !u.mesh.vertices.is_empty())
            {
                anyhow::ensure!(
                    u.range.vertex_offset >= 0
                        && (u.range.vertex_offset as usize + u.mesh.vertices.len())
                            * std::mem::size_of::<Vertex>()
                            <= self.vertex.size as usize,
                    "Streaming vertex arena overflow"
                );
                anyhow::ensure!(
                    (u.range.first_index as usize + u.mesh.indices.len()) * 4
                        <= self.index.size as usize,
                    "Streaming index arena overflow"
                );
                let v = bytemuck::cast_slice(&u.mesh.vertices);
                let i = bytemuck::cast_slice(&u.mesh.indices);
                f.staging.write(0, v)?;
                f.staging.write(v.len(), i)?;
                let before = vk::MemoryBarrier::default()
                    .src_access_mask(
                        vk::AccessFlags::VERTEX_ATTRIBUTE_READ | vk::AccessFlags::INDEX_READ,
                    )
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::VERTEX_INPUT,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[before],
                    &[],
                    &[],
                );
                device.cmd_copy_buffer(
                    cmd,
                    f.staging.handle,
                    self.vertex.handle,
                    &[vk::BufferCopy {
                        src_offset: 0,
                        dst_offset: u.range.vertex_offset as u64
                            * std::mem::size_of::<Vertex>() as u64,
                        size: v.len() as u64,
                    }],
                );
                device.cmd_copy_buffer(
                    cmd,
                    f.staging.handle,
                    self.index.handle,
                    &[vk::BufferCopy {
                        src_offset: v.len() as u64,
                        dst_offset: u.range.first_index as u64 * 4,
                        size: i.len() as u64,
                    }],
                );
                let after = vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(
                        vk::AccessFlags::VERTEX_ATTRIBUTE_READ | vk::AccessFlags::INDEX_READ,
                    );
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::VERTEX_INPUT,
                    vk::DependencyFlags::empty(),
                    &[after],
                    &[],
                    &[],
                );
            }
            if let Some(pipeline) = self.probe_pipeline {
                let before = vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE);
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::FRAGMENT_SHADER
                        | vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[before],
                    &[],
                    &[],
                );
                device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::COMPUTE,
                    self.layout,
                    0,
                    &[f.set],
                    &[],
                );
                device.cmd_push_constants(
                    cmd,
                    self.layout,
                    vk::ShaderStageFlags::ALL,
                    0,
                    bytemuck::cast_slice(&[self.probe_frame, 0u32, 0, 0]),
                );
                // One workgroup per updated probe; 32 lanes trace its 64 rays in parallel.
                // Keep the 256-probe cadence in sync with probe-update.comp.
                device.cmd_dispatch(cmd, 256, 1, 1);
            }
            if let Some(pool) = self.timing {
                device.cmd_write_timestamp(
                    cmd,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    pool,
                    self.frame as u32 * 6 + 1,
                );
            }
            image_barrier(
                &self.ctx,
                cmd,
                self.shadow.handle,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                vk::ImageAspectFlags::DEPTH,
                0,
                1,
                vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            );
            let depth = vk::RenderingAttachmentInfo::default()
                .image_view(self.shadow.view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.,
                        stencil: 0,
                    },
                });
            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent: vk::Extent2D {
                            width: 4096,
                            height: 2048,
                        },
                    })
                    .layer_count(1)
                    .depth_attachment(&depth),
            );
            self.bind(
                cmd,
                f,
                vk::Extent2D {
                    width: 2048,
                    height: 2048,
                },
                self.shadow_pipeline,
            );
            self.draw_list(cmd, shadow_draws, 1);
            device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 2048.,
                    y: 0.,
                    width: 2048.,
                    height: 2048.,
                    min_depth: 0.,
                    max_depth: 1.,
                }],
            );
            device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: 2048, y: 0 },
                    extent: vk::Extent2D {
                        width: 2048,
                        height: 2048,
                    },
                }],
            );
            self.draw_list(cmd, far_shadow_draws, 2);
            device.cmd_end_rendering(cmd);
            if let Some(pool) = self.timing {
                device.cmd_write_timestamp(
                    cmd,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    pool,
                    self.frame as u32 * 6 + 2,
                );
            }
            image_barrier(
                &self.ctx,
                cmd,
                self.shadow.handle,
                vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                vk::ImageAspectFlags::DEPTH,
                0,
                1,
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
            );
            // Shadows do not read diffuse probes. Delay their dependency until
            // surface shading so compute can overlap independent shadow work.
            if self.probe_pipeline.is_some() {
                let after = vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ);
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[after],
                    &[],
                    &[],
                );
            }
            image_barrier(
                &self.ctx,
                cmd,
                self.swap.images[image as usize],
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
                0,
                1,
                vk::AccessFlags::empty(),
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            );
            image_barrier(
                &self.ctx,
                cmd,
                self.swap.color.handle,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
                0,
                1,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            );
            image_barrier(
                &self.ctx,
                cmd,
                self.swap.depth.handle,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                vk::ImageAspectFlags::DEPTH,
                0,
                1,
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE | vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            );
            image_barrier(
                &self.ctx,
                cmd,
                self.swap.hdr.handle,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
                0,
                1,
                vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            );
            let colors = [vk::RenderingAttachmentInfo::default()
                .image_view(self.swap.color.view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .resolve_mode(vk::ResolveModeFlags::AVERAGE)
                .resolve_image_view(self.swap.hdr.view)
                .resolve_image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.08, 0.15, 0.24, 1.],
                    },
                })];
            let depth = vk::RenderingAttachmentInfo::default()
                .image_view(self.swap.depth.view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.,
                        stencil: 0,
                    },
                });
            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent: self.swap.extent,
                    })
                    .layer_count(1)
                    .color_attachments(&colors)
                    .depth_attachment(&depth),
            );
            self.bind(cmd, f, self.swap.extent, self.sky_pipeline);
            device.cmd_draw(cmd, 3, 1, 0, 0);
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            self.draw_list(cmd, draws, 0);
            device.cmd_end_rendering(cmd);
            if !water_draws.is_empty() {
                image_barrier(
                    &self.ctx,
                    cmd,
                    self.swap.hdr.handle,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageAspectFlags::COLOR,
                    0,
                    1,
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::PipelineStageFlags::TRANSFER,
                );
                image_barrier(
                    &self.ctx,
                    cmd,
                    self.swap.scene_color.handle,
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageAspectFlags::COLOR,
                    0,
                    1,
                    vk::AccessFlags::SHADER_READ,
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::PipelineStageFlags::TRANSFER,
                );
                let sub = vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1);
                device.cmd_copy_image(
                    cmd,
                    self.swap.hdr.handle,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    self.swap.scene_color.handle,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::ImageCopy::default()
                        .src_subresource(sub)
                        .dst_subresource(sub)
                        .extent(vk::Extent3D {
                            width: self.swap.extent.width,
                            height: self.swap.extent.height,
                            depth: 1,
                        })],
                );
                image_barrier(
                    &self.ctx,
                    cmd,
                    self.swap.scene_color.handle,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::ImageAspectFlags::COLOR,
                    0,
                    1,
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::AccessFlags::SHADER_READ,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                );
                image_barrier(
                    &self.ctx,
                    cmd,
                    self.swap.hdr.handle,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageAspectFlags::COLOR,
                    0,
                    1,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                );
                image_barrier(
                    &self.ctx,
                    cmd,
                    self.swap.color.handle,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageAspectFlags::COLOR,
                    0,
                    1,
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    vk::AccessFlags::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                );
                image_barrier(
                    &self.ctx,
                    cmd,
                    self.swap.depth.handle,
                    vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                    vk::ImageAspectFlags::DEPTH,
                    0,
                    1,
                    vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    vk::AccessFlags::SHADER_READ | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
                    vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    vk::PipelineStageFlags::FRAGMENT_SHADER
                        | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                );
                let water_colors = [colors[0].load_op(vk::AttachmentLoadOp::LOAD)];
                let water_depth = depth
                    .load_op(vk::AttachmentLoadOp::LOAD)
                    .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL);
                device.cmd_begin_rendering(
                    cmd,
                    &vk::RenderingInfo::default()
                        .render_area(vk::Rect2D {
                            offset: vk::Offset2D::default(),
                            extent: self.swap.extent,
                        })
                        .layer_count(1)
                        .color_attachments(&water_colors)
                        .depth_attachment(&water_depth),
                );
                self.bind(cmd, f, self.swap.extent, self.water_pipeline);
                self.draw_list(cmd, water_draws, 0);
                device.cmd_end_rendering(cmd);
            }
            image_barrier(
                &self.ctx,
                cmd,
                self.swap.hdr.handle,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
                0,
                1,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
            );
            if water_draws.is_empty() {
                image_barrier(
                    &self.ctx,
                    cmd,
                    self.swap.depth.handle,
                    vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                    vk::ImageAspectFlags::DEPTH,
                    0,
                    1,
                    vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    vk::AccessFlags::SHADER_READ,
                    vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                );
            }
            image_barrier(
                &self.ctx,
                cmd,
                self.swap.volume.handle,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
                0,
                1,
                vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            );
            if let Some(pool) = self.timing {
                device.cmd_write_timestamp(
                    cmd,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    pool,
                    self.frame as u32 * 6 + 3,
                );
            }
            let volume_extent = vk::Extent2D {
                width: self.swap.extent.width.div_ceil(2),
                height: self.swap.extent.height.div_ceil(2),
            };
            let volume_attachment = [vk::RenderingAttachmentInfo::default()
                .image_view(self.swap.volume.view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE)];
            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent: volume_extent,
                    })
                    .layer_count(1)
                    .color_attachments(&volume_attachment),
            );
            self.bind(cmd, f, volume_extent, self.volume_pipeline);
            device.cmd_draw(cmd, 3, 1, 0, 0);
            device.cmd_end_rendering(cmd);
            if let Some(pool) = self.timing {
                device.cmd_write_timestamp(
                    cmd,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    pool,
                    self.frame as u32 * 6 + 4,
                );
            }
            image_barrier(
                &self.ctx,
                cmd,
                self.swap.volume.handle,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
                0,
                1,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
            );
            let output = [vk::RenderingAttachmentInfo::default()
                .image_view(self.swap.views[image as usize])
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE)];
            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent: self.swap.extent,
                    })
                    .layer_count(1)
                    .color_attachments(&output),
            );
            self.bind(cmd, f, self.swap.extent, self.tonemap_pipeline);
            device.cmd_draw(cmd, 3, 1, 0, 0);
            device.cmd_end_rendering(cmd);
            if let Some(pool) = self.timing {
                device.cmd_write_timestamp(
                    cmd,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    pool,
                    self.frame as u32 * 6 + 5,
                );
            }
            let capture_buffer = if capture.is_some() {
                let b = Buffer::new(
                    &self.ctx,
                    (self.swap.extent.width * self.swap.extent.height * 4) as u64,
                    vk::BufferUsageFlags::TRANSFER_DST,
                    true,
                )?;
                image_barrier(
                    &self.ctx,
                    cmd,
                    self.swap.images[image as usize],
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageAspectFlags::COLOR,
                    0,
                    1,
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::PipelineStageFlags::TRANSFER,
                );
                device.cmd_copy_image_to_buffer(
                    cmd,
                    self.swap.images[image as usize],
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    b.handle,
                    &[vk::BufferImageCopy::default()
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .layer_count(1),
                        )
                        .image_extent(vk::Extent3D {
                            width: self.swap.extent.width,
                            height: self.swap.extent.height,
                            depth: 1,
                        })],
                );
                Some(b)
            } else {
                None
            };
            image_barrier(
                &self.ctx,
                cmd,
                self.swap.images[image as usize],
                if capture.is_some() {
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL
                } else {
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                },
                vk::ImageLayout::PRESENT_SRC_KHR,
                vk::ImageAspectFlags::COLOR,
                0,
                1,
                if capture.is_some() {
                    vk::AccessFlags::TRANSFER_READ
                } else {
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                },
                vk::AccessFlags::empty(),
                if capture.is_some() {
                    vk::PipelineStageFlags::TRANSFER
                } else {
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                },
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            );
            device.end_command_buffer(cmd)?;
            let waits = [f.acquire];
            let stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let signals = [self.swap.present[image as usize]];
            let commands = [cmd];
            device.queue_submit(
                self.ctx.queue,
                &[vk::SubmitInfo::default()
                    .wait_semaphores(&waits)
                    .wait_dst_stage_mask(&stages)
                    .command_buffers(&commands)
                    .signal_semaphores(&signals)],
                f.fence,
            )?;
            let chains = [self.swap.handle];
            let indices = [image];
            let presented = self.swap.api.queue_present(
                self.ctx.queue,
                &vk::PresentInfoKHR::default()
                    .wait_semaphores(&signals)
                    .swapchains(&chains)
                    .image_indices(&indices),
            );
            if let (Some(path), Some(buffer)) = (capture, capture_buffer) {
                device.wait_for_fences(&[f.fence], true, u64::MAX)?;
                let mut pixels = std::slice::from_raw_parts(
                    buffer.mapped.unwrap().as_ptr(),
                    buffer.size as usize,
                )
                .to_vec();
                if self.swap.format == vk::Format::B8G8R8A8_SRGB
                    || self.swap.format == vk::Format::B8G8R8A8_UNORM
                {
                    for p in pixels.as_chunks_mut::<4>().0 {
                        p.swap(0, 2);
                    }
                }
                image::save_buffer(
                    path,
                    &pixels,
                    self.swap.extent.width,
                    self.swap.extent.height,
                    image::ColorType::Rgba8,
                )?;
                println!("Screenshot {}", path.display());
            }
            self.probe_frame = self.probe_frame.wrapping_add(1);
            self.frame = (self.frame + 1) % 2;
            match presented {
                Ok(changed) => Ok(!changed && !suboptimal),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(false),
                Err(e) => Err(e.into()),
            }
        }
    }
    unsafe fn bind(
        &self,
        cmd: vk::CommandBuffer,
        f: &Frame,
        extent: vk::Extent2D,
        pipeline: vk::Pipeline,
    ) {
        unsafe {
            let d = &self.ctx.device;
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            d.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout,
                0,
                &[f.set],
                &[],
            );
            d.cmd_bind_vertex_buffers(cmd, 0, &[self.vertex.handle, f.instances.handle], &[0, 0]);
            d.cmd_bind_index_buffer(cmd, self.index.handle, 0, vk::IndexType::UINT32);
            d.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.,
                    y: 0.,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.,
                    max_depth: 1.,
                }],
            );
            d.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent,
                }],
            );
        }
    }
    unsafe fn draw_list(&self, cmd: vk::CommandBuffer, draws: &[Draw], pass: u32) {
        unsafe {
            let shadow = pass == 1 || pass == 2;
            for phase in 0..if shadow { 2 } else { 1 } {
                if shadow {
                    self.ctx.device.cmd_bind_pipeline(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        if phase == 0 {
                            self.opaque_shadow_pipeline
                        } else {
                            self.shadow_pipeline
                        },
                    );
                }
                for d in draws {
                    if d.instances == 0
                        || (shadow && self.solid_materials[d.material as usize] != (phase == 0))
                    {
                        continue;
                    }
                    self.ctx.device.cmd_push_constants(
                        cmd,
                        self.layout,
                        vk::ShaderStageFlags::ALL,
                        0,
                        bytemuck::cast_slice(&[d.material, pass, 0u32, 0u32]),
                    );
                    self.ctx.device.cmd_draw_indexed(
                        cmd,
                        d.mesh.index_count,
                        d.instances,
                        d.mesh.first_index,
                        d.mesh.vertex_offset,
                        d.first_instance,
                    );
                }
            }
        }
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            let d = &self.ctx.device;
            let _ = d.device_wait_idle();
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_pipeline(self.water_pipeline, None);
            d.destroy_pipeline(self.shadow_pipeline, None);
            d.destroy_pipeline(self.opaque_shadow_pipeline, None);
            d.destroy_pipeline(self.sky_pipeline, None);
            d.destroy_pipeline(self.tonemap_pipeline, None);
            d.destroy_pipeline(self.volume_pipeline, None);
            d.destroy_pipeline_layout(self.layout, None);
            if let Some(pipeline) = self.probe_pipeline {
                d.destroy_pipeline(pipeline, None);
            }
            if let Some(pool) = self.timing {
                d.destroy_query_pool(pool, None);
            }
            d.destroy_descriptor_pool(self.descriptor_pool, None);
            d.destroy_descriptor_set_layout(self.set_layout, None);
            for f in &self.frames {
                d.destroy_fence(f.fence, None);
                d.destroy_semaphore(f.acquire, None);
            }
            d.destroy_command_pool(self.pool, None);
            d.destroy_sampler(self.sampler, None);
            d.destroy_sampler(self.shadow_sampler, None);
            destroy_swapchain(&self.ctx, &self.swap);
        }
    }
}

fn make_swapchain(ctx: &Arc<Context>, window: &Window, old: vk::SwapchainKHR) -> Result<Swapchain> {
    unsafe {
        let api = ash::khr::swapchain::Device::new(&ctx.instance, &ctx.device);
        let caps = ctx
            .surface_api
            .get_physical_device_surface_capabilities(ctx.physical, ctx.surface)?;
        let formats = ctx
            .surface_api
            .get_physical_device_surface_formats(ctx.physical, ctx.surface)?;
        let format = formats
            .iter()
            .find(|f| f.format == vk::Format::B8G8R8A8_SRGB)
            .copied()
            .unwrap_or(formats[0]);
        let size = window.inner_size();
        let extent = if caps.current_extent.width != u32::MAX {
            caps.current_extent
        } else {
            vk::Extent2D {
                width: size
                    .width
                    .clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: size
                    .height
                    .clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        };
        let mut count = caps.min_image_count + 1;
        if caps.max_image_count > 0 {
            count = count.min(caps.max_image_count);
        }
        let modes = ctx
            .surface_api
            .get_physical_device_surface_present_modes(ctx.physical, ctx.surface)?;
        let mode = if modes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else {
            vk::PresentModeKHR::FIFO
        };
        let handle = api.create_swapchain(
            &vk::SwapchainCreateInfoKHR::default()
                .surface(ctx.surface)
                .min_image_count(count)
                .image_format(format.format)
                .image_color_space(format.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
                )
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(mode)
                .clipped(true)
                .old_swapchain(old),
            None,
        )?;
        let images = api.get_swapchain_images(handle)?;
        let mut views = Vec::new();
        let mut present = Vec::new();
        for &image in &images {
            views.push(
                ctx.device.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format.format)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .level_count(1)
                                .layer_count(1),
                        ),
                    None,
                )?,
            );
            present.push(
                ctx.device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?,
            );
        }
        let depth = Image::new(
            ctx,
            extent.width,
            extent.height,
            vk::Format::D32_SFLOAT,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            1,
            vk::SampleCountFlags::TYPE_4,
            vk::ImageAspectFlags::DEPTH,
        )?;
        let color = Image::new(
            ctx,
            extent.width,
            extent.height,
            HDR_FORMAT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
            1,
            vk::SampleCountFlags::TYPE_4,
            vk::ImageAspectFlags::COLOR,
        )?;
        let scene_color = Image::new(
            ctx,
            extent.width,
            extent.height,
            HDR_FORMAT,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            1,
            vk::SampleCountFlags::TYPE_1,
            vk::ImageAspectFlags::COLOR,
        )?;
        let hdr = Image::new(
            ctx,
            extent.width,
            extent.height,
            HDR_FORMAT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::SAMPLED,
            1,
            vk::SampleCountFlags::TYPE_1,
            vk::ImageAspectFlags::COLOR,
        )?;
        let volume = Image::new(
            ctx,
            extent.width.div_ceil(2),
            extent.height.div_ceil(2),
            HDR_FORMAT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            1,
            vk::SampleCountFlags::TYPE_1,
            vk::ImageAspectFlags::COLOR,
        )?;
        Ok(Swapchain {
            api,
            handle,
            images,
            views,
            present,
            extent,
            format: format.format,
            depth,
            color,
            scene_color,
            hdr,
            volume,
        })
    }
}
unsafe fn destroy_swapchain(ctx: &Context, swap: &Swapchain) {
    unsafe {
        for &v in &swap.views {
            ctx.device.destroy_image_view(v, None);
        }
        for &s in &swap.present {
            ctx.device.destroy_semaphore(s, None);
        }
        swap.api.destroy_swapchain(swap.handle, None);
    }
}

fn immediate(
    ctx: &Context,
    pool: vk::CommandPool,
    record: impl FnOnce(vk::CommandBuffer),
) -> Result<()> {
    unsafe {
        let cmd = ctx.device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0];
        ctx.device.begin_command_buffer(
            cmd,
            &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        record(cmd);
        ctx.device.end_command_buffer(cmd)?;
        let fence = ctx
            .device
            .create_fence(&vk::FenceCreateInfo::default(), None)?;
        ctx.device.queue_submit(
            ctx.queue,
            &[vk::SubmitInfo::default().command_buffers(&[cmd])],
            fence,
        )?;
        ctx.device.wait_for_fences(&[fence], true, u64::MAX)?;
        ctx.device.destroy_fence(fence, None);
        ctx.device.free_command_buffers(pool, &[cmd]);
        Ok(())
    }
}
fn upload_buffer(
    ctx: &Arc<Context>,
    pool: vk::CommandPool,
    target: &Buffer,
    bytes: &[u8],
) -> Result<()> {
    let staging = Buffer::new(
        ctx,
        bytes.len() as u64,
        vk::BufferUsageFlags::TRANSFER_SRC,
        true,
    )?;
    staging.write(0, bytes)?;
    immediate(ctx, pool, |cmd| unsafe {
        ctx.device.cmd_copy_buffer(
            cmd,
            staging.handle,
            target.handle,
            &[vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: bytes.len() as u64,
            }],
        );
        let barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(
                vk::AccessFlags::VERTEX_ATTRIBUTE_READ
                    | vk::AccessFlags::INDEX_READ
                    | vk::AccessFlags::SHADER_READ,
            );
        ctx.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::VERTEX_INPUT
                | vk::PipelineStageFlags::FRAGMENT_SHADER
                | if ctx.ray_query {
                    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR
                } else {
                    vk::PipelineStageFlags::empty()
                },
            vk::DependencyFlags::empty(),
            &[barrier],
            &[],
            &[],
        );
    })
}
fn upload_texture(
    ctx: &Arc<Context>,
    pool: vk::CommandPool,
    source: &TextureData,
) -> Result<Image> {
    let levels = 32 - source.width.max(source.height).leading_zeros();
    let format = if source.srgb {
        vk::Format::R8G8B8A8_SRGB
    } else {
        vk::Format::R8G8B8A8_UNORM
    };
    let image = Image::new(
        ctx,
        source.width,
        source.height,
        format,
        vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_DST
            | vk::ImageUsageFlags::TRANSFER_SRC,
        levels,
        vk::SampleCountFlags::TYPE_1,
        vk::ImageAspectFlags::COLOR,
    )?;
    if !source.mips.is_empty() {
        let total = source.pixels.len() + source.mips.iter().map(Vec::len).sum::<usize>();
        let staging = Buffer::new(ctx, total as u64, vk::BufferUsageFlags::TRANSFER_SRC, true)?;
        let (mut w, mut h, mut offset) = (source.width, source.height, 0_u64);
        let mut copies = Vec::with_capacity(levels as usize);
        for (level, bytes) in std::iter::once(&source.pixels)
            .chain(&source.mips)
            .enumerate()
        {
            staging.write(offset as usize, bytes)?;
            copies.push(
                vk::BufferImageCopy::default()
                    .buffer_offset(offset)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .mip_level(level as u32)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D {
                        width: w,
                        height: h,
                        depth: 1,
                    }),
            );
            offset += bytes.len() as u64;
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        immediate(ctx, pool, |cmd| unsafe {
            image_barrier(
                ctx,
                cmd,
                image.handle,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
                0,
                levels,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
            );
            ctx.device.cmd_copy_buffer_to_image(
                cmd,
                staging.handle,
                image.handle,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &copies,
            );
            image_barrier(
                ctx,
                cmd,
                image.handle,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
                0,
                levels,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
            );
        })?;
        return Ok(image);
    }
    let staging = Buffer::new(
        ctx,
        source.pixels.len() as u64,
        vk::BufferUsageFlags::TRANSFER_SRC,
        true,
    )?;
    staging.write(0, &source.pixels)?;
    immediate(ctx, pool, |cmd| unsafe {
        image_barrier(
            ctx,
            cmd,
            image.handle,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageAspectFlags::COLOR,
            0,
            levels,
            vk::AccessFlags::empty(),
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
        );
        ctx.device.cmd_copy_buffer_to_image(
            cmd,
            staging.handle,
            image.handle,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::BufferImageCopy::default()
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1),
                )
                .image_extent(vk::Extent3D {
                    width: source.width,
                    height: source.height,
                    depth: 1,
                })],
        );
        let (mut w, mut h) = (source.width as i32, source.height as i32);
        for level in 1..levels {
            image_barrier(
                ctx,
                cmd,
                image.handle,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
                level - 1,
                1,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::TRANSFER_READ,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
            );
            let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
            let region = vk::ImageBlit::default()
                .src_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .mip_level(level - 1)
                        .layer_count(1),
                )
                .src_offsets([vk::Offset3D::default(), vk::Offset3D { x: w, y: h, z: 1 }])
                .dst_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .mip_level(level)
                        .layer_count(1),
                )
                .dst_offsets([vk::Offset3D::default(), vk::Offset3D { x: nw, y: nh, z: 1 }]);
            ctx.device.cmd_blit_image(
                cmd,
                image.handle,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image.handle,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
                vk::Filter::LINEAR,
            );
            image_barrier(
                ctx,
                cmd,
                image.handle,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
                level - 1,
                1,
                vk::AccessFlags::TRANSFER_READ,
                vk::AccessFlags::SHADER_READ,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER,
            );
            w = nw;
            h = nh;
        }
        image_barrier(
            ctx,
            cmd,
            image.handle,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageAspectFlags::COLOR,
            levels - 1,
            1,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::SHADER_READ,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::VERTEX_SHADER
                | vk::PipelineStageFlags::FRAGMENT_SHADER
                | vk::PipelineStageFlags::COMPUTE_SHADER,
        );
    })?;
    Ok(image)
}

#[allow(clippy::too_many_arguments)]
fn make_pipeline(
    ctx: &Context,
    layout: vk::PipelineLayout,
    vertex: &[u8],
    fragment: &[u8],
    format: Option<vk::Format>,
    samples: vk::SampleCountFlags,
    shadow: bool,
    sky: bool,
    depth_write: bool,
) -> Result<vk::Pipeline> {
    unsafe {
        let vs = ctx.device.create_shader_module(
            &vk::ShaderModuleCreateInfo::default()
                .code(&ash::util::read_spv(&mut Cursor::new(vertex))?),
            None,
        )?;
        // An empty fragment module requests a true depth-only pipeline.
        let fs = if fragment.is_empty() {
            None
        } else {
            Some(
                ctx.device.create_shader_module(
                    &vk::ShaderModuleCreateInfo::default()
                        .code(&ash::util::read_spv(&mut Cursor::new(fragment))?),
                    None,
                )?,
            )
        };
        let mut stages = vec![
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vs)
                .name(c"main"),
        ];
        if let Some(fs) = fs {
            stages.push(
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fs)
                    .name(c"main"),
            );
        }
        let bindings = [
            vk::VertexInputBindingDescription {
                binding: 0,
                stride: 56,
                input_rate: vk::VertexInputRate::VERTEX,
            },
            vk::VertexInputBindingDescription {
                binding: 1,
                stride: 48,
                input_rate: vk::VertexInputRate::INSTANCE,
            },
        ];
        let attributes = [
            (0, 0, vk::Format::R32G32B32_SFLOAT, 0),
            (1, 0, vk::Format::R32G32B32_SFLOAT, 12),
            (2, 0, vk::Format::R32G32_SFLOAT, 24),
            (3, 0, vk::Format::R32G32B32A32_SFLOAT, 32),
            (4, 0, vk::Format::R32G32_SFLOAT, 48),
            (5, 1, vk::Format::R32G32B32A32_SFLOAT, 0),
            (6, 1, vk::Format::R32G32B32A32_SFLOAT, 16),
            (7, 1, vk::Format::R32G32B32A32_SFLOAT, 32),
        ]
        .map(
            |(location, binding, format, offset)| vk::VertexInputAttributeDescription {
                location,
                binding,
                format,
                offset,
            },
        );
        let input = if sky {
            vk::PipelineVertexInputStateCreateInfo::default()
        } else {
            vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_binding_descriptions(&bindings)
                .vertex_attribute_descriptions(&attributes)
        };
        let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.)
            .depth_bias_enable(shadow)
            .depth_bias_constant_factor(1.5)
            .depth_bias_slope_factor(1.5);
        let msaa = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(samples)
            .alpha_to_coverage_enable(!shadow && !sky && depth_write);
        let depth = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(!sky)
            .depth_write_enable(depth_write)
            .depth_compare_op(vk::CompareOp::LESS);
        let attachments = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)];
        let blend = vk::PipelineColorBlendStateCreateInfo::default()
            .attachments(if format.is_some() { &attachments } else { &[] });
        let dynamic = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
        let color_formats: Vec<_> = format.into_iter().collect();
        let mut render = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&color_formats)
            .depth_attachment_format(if sky && samples == vk::SampleCountFlags::TYPE_1 {
                vk::Format::UNDEFINED
            } else {
                vk::Format::D32_SFLOAT
            });
        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&input)
            .input_assembly_state(&assembly)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&msaa)
            .depth_stencil_state(&depth)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(layout)
            .push_next(&mut render);
        let result = ctx
            .device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
            .map_err(|(_, e)| anyhow::anyhow!("Vulkan pipeline: {e:?}"));
        ctx.device.destroy_shader_module(vs, None);
        if let Some(fs) = fs {
            ctx.device.destroy_shader_module(fs, None);
        }
        Ok(result?[0])
    }
}

fn bake_transport(
    ctx: &Arc<Context>,
    pool: vk::CommandPool,
    layout: vk::PipelineLayout,
    set: vk::DescriptorSet,
    code: &[u8],
    bake: TransportBake,
) -> Result<()> {
    let start = std::time::Instant::now();
    unsafe {
        let words = ash::util::read_spv(&mut Cursor::new(code))?;
        let module = ctx
            .device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)?;
        let pipeline = ctx
            .device
            .create_compute_pipelines(
                vk::PipelineCache::null(),
                &[vk::ComputePipelineCreateInfo::default()
                    .layout(layout)
                    .stage(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .module(module)
                            .name(c"main"),
                    )],
                None,
            )
            .map_err(|(_, e)| e)?[0];
        let result = immediate(ctx, pool, |cmd| {
            ctx.device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
            ctx.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                layout,
                0,
                &[set],
                &[],
            );
            ctx.device.cmd_push_constants(
                cmd,
                layout,
                vk::ShaderStageFlags::ALL,
                0,
                bytemuck::cast_slice(&[bake.occupancy, bake.heights, bake.size, bake.layers]),
            );
            ctx.device.cmd_dispatch(
                cmd,
                bake.size.div_ceil(8),
                bake.size.div_ceil(8),
                bake.layers,
            );
            let barrier = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);
            ctx.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[barrier],
                &[],
                &[],
            );
        });
        ctx.device.destroy_pipeline(pipeline, None);
        ctx.device.destroy_shader_module(module, None);
        result?;
    }
    println!(
        "Full-map Vulkan transport bake: {:.1} ms, {} probes",
        start.elapsed().as_secs_f64() * 1000.,
        bake.size * bake.size * bake.layers
    );
    Ok(())
}

fn make_compute_pipeline(
    ctx: &Arc<Context>,
    layout: vk::PipelineLayout,
    code: &[u8],
) -> Result<vk::Pipeline> {
    unsafe {
        let words = ash::util::read_spv(&mut Cursor::new(code))?;
        let module = ctx
            .device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)?;
        let result = ctx.device.create_compute_pipelines(
            vk::PipelineCache::null(),
            &[vk::ComputePipelineCreateInfo::default()
                .layout(layout)
                .stage(
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::COMPUTE)
                        .module(module)
                        .name(c"main"),
                )],
            None,
        );
        ctx.device.destroy_shader_module(module, None);
        Ok(result.map_err(|(_, e)| e)?[0])
    }
}
