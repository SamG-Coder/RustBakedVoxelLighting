//! Browser WebGPU renderer. Native Vulkan, RTX and filesystem APIs are compiled out.
use crate::assets::*;
pub use crate::render_types::*;
use std::collections::BTreeMap;
use wgpu::util::DeviceExt;
struct Page {
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    first: u32,
}
struct FrameTargets {
    msaa: wgpu::TextureView,
    opaque: wgpu::TextureView,
    hdr: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolved_depth: wgpu::TextureView,
    volume: wgpu::TextureView,
    opaque_inputs: wgpu::BindGroup,
    water_inputs: wgpu::BindGroup,
    display_inputs: wgpu::BindGroup,
    volume_inputs: wgpu::BindGroup,
    depth_inputs: wgpu::BindGroup,
}
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    uniform: wgpu::Buffer,
    instances: wgpu::Buffer,
    global: wgpu::BindGroup,
    lighting_layout: wgpu::BindGroupLayout,
    lighting: Option<wgpu::BindGroup>,
    shadow_views: [wgpu::TextureView; 2],
    shadow_pipelines: [wgpu::RenderPipeline; 2],
    material_layout: wgpu::BindGroupLayout,
    materials: Vec<wgpu::BindGroup>,
    textures: Vec<wgpu::TextureView>,
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    water: wgpu::RenderPipeline,
    sky: wgpu::RenderPipeline,
    vertex: Option<wgpu::Buffer>,
    index: Option<wgpu::Buffer>,
    pages: BTreeMap<i32, Page>,
    targets: FrameTargets,
    post_layout: wgpu::BindGroupLayout,
    depth_layout: wgpu::BindGroupLayout,
    depth_resolve: wgpu::RenderPipeline,
    volume_pipeline: wgpu::RenderPipeline,
    display_pipeline: wgpu::RenderPipeline,
}
impl Renderer {
    pub async fn new(canvas: web_sys::HtmlCanvasElement) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Woodland WebGPU"),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        device.on_uncaptured_error(Box::new(|e| {
            web_sys::console::error_1(&e.to_string().into())
        }));
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or("No surface format")?;
        // Browser canvas formats are normally UNORM. Declare the compatible sRGB
        // view explicitly, otherwise linear HDR output is displayed without encoding.
        config.view_formats = vec![config.format.add_srgb_suffix()];
        web_sys::console::info_1(
            &format!(
                "Canvas {:?}, display view {:?}",
                config.format, config.view_formats[0]
            )
            .into(),
        );
        surface.configure(&device, &config);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Frame globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Retained instances"),
            size: 200000 * 48,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let global_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let global = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &global_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let mut entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        for binding in 2..10 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
        let mut le = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }];
        for binding in 1..8 {
            let ty = match binding {
                3 => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                5 => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                _ => wgpu::BindingType::Texture {
                    sample_type: if binding < 3 {
                        wgpu::TextureSampleType::Depth
                    } else {
                        wgpu::TextureSampleType::Float { filterable: true }
                    },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            };
            le.push(wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty,
                count: None,
            });
        }
        let lighting_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Transport and shadows"),
            entries: &le,
        });
        let shadow_views = [
            Self::depth(&device, 2048, 2048),
            Self::depth(&device, 2048, 2048),
        ];
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene inputs"),
            entries: &[0, 1, 2, 3].map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: match binding {
                    2 => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    _ => wgpu::BindingType::Texture {
                        sample_type: if binding == 1 {
                            wgpu::TextureSampleType::Depth
                        } else {
                            wgpu::TextureSampleType::Float { filterable: true }
                        },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                },
                count: None,
            }),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                &global_layout,
                &material_layout,
                &lighting_layout,
                &post_layout,
            ],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Woodland surfaces"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\n{}\n{}",
                    include_str!("web_surface.wgsl"),
                    include_str!("web_lighting.wgsl"),
                    include_str!("web_post.wgsl")
                )
                .into(),
            ),
        });
        let va = wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Float32x4,4=>Float32x2];
        let ia = wgpu::vertex_attr_array![5=>Float32x4,6=>Float32x4,7=>Float32x4];
        let buffers = [
            wgpu::VertexBufferLayout {
                array_stride: 56,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &va,
            },
            wgpu::VertexBufferLayout {
                array_stride: 48,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &ia,
            },
        ];
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow casters"),
            bind_group_layouts: &[&global_layout, &material_layout],
            push_constant_ranges: &[],
        });
        let shadow_pipelines = ["shadow_vs", "far_shadow_vs"].map(|entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&shadow_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(entry),
                    buffers: &buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("shadow_fs"),
                    targets: &[],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        });
        let make = |water: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if water { "River" } else { "World" }),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: !water,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: 4,
                    alpha_to_coverage_enabled: !water,
                    ..Default::default()
                },
                multiview: None,
                cache: None,
            })
        };
        let pipeline = make(false);
        let water = make(true);
        let sky_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&global_layout],
            push_constant_ranges: &[],
        });
        let sky = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky"),
            layout: Some(&sky_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("sky_vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("sky_fs"),
                targets: &[Some(wgpu::TextureFormat::Rgba16Float.into())],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 4,
                ..Default::default()
            },
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Trilinear anisotropic"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 16,
            ..Default::default()
        });
        let fullscreen = |entry, format| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("sky_vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let volume_pipeline = fullscreen("volume_fs", wgpu::TextureFormat::Rgba16Float);
        let display_pipeline = fullscreen("display_fs", config.format.add_srgb_suffix());
        let depth_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Depth resolve"),
            source: wgpu::ShaderSource::Wgsl(include_str!("web_depth.wgsl").into()),
        });
        let depth_resolve = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Depth resolve"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &depth_shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &depth_shader,
                entry_point: Some("fs"),
                targets: &[],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let depth_layout = depth_resolve.get_bind_group_layout(0);
        let targets = Self::targets(&device, width, height, &post_layout, &depth_layout);
        if let Some(error) = device.pop_error_scope().await {
            return Err(error.to_string());
        }
        Ok(Self {
            device,
            queue,
            surface,
            config,
            uniform,
            instances,
            global,
            lighting_layout,
            lighting: None,
            shadow_views,
            shadow_pipelines,
            material_layout,
            materials: Vec::new(),
            textures: Vec::new(),
            sampler,
            pipeline,
            water,
            sky,
            vertex: None,
            index: None,
            pages: BTreeMap::new(),
            targets,
            post_layout,
            depth_layout,
            depth_resolve,
            volume_pipeline,
            display_pipeline,
        })
    }
    fn targets(
        d: &wgpu::Device,
        w: u32,
        h: u32,
        layout: &wgpu::BindGroupLayout,
        depth_layout: &wgpu::BindGroupLayout,
    ) -> FrameTargets {
        let image = |label, width, height, format, samples| {
            d.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
        };
        let msaa = image("HDR MSAA", w, h, wgpu::TextureFormat::Rgba16Float, 4);
        let opaque = image("Opaque HDR", w, h, wgpu::TextureFormat::Rgba16Float, 1);
        let hdr = image("Composite HDR", w, h, wgpu::TextureFormat::Rgba16Float, 1);
        let depth = image("MSAA depth", w, h, wgpu::TextureFormat::Depth32Float, 4);
        let resolved_depth = image("Scene depth", w, h, wgpu::TextureFormat::Depth32Float, 1);
        let volume = image(
            "Half resolution aerial scattering",
            w.div_ceil(2),
            h.div_ceil(2),
            wgpu::TextureFormat::Rgba16Float,
            1,
        );
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let inputs = |colour: &wgpu::TextureView, vol: &wgpu::TextureView| {
            d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Scene inputs"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(colour),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&resolved_depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(vol),
                    },
                ],
            })
        };
        let opaque_inputs = inputs(&hdr, &volume);
        let water_inputs = inputs(&opaque, &volume);
        let display_inputs = inputs(&hdr, &volume);
        let volume_inputs = inputs(&opaque, &hdr);
        let depth_inputs = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("MSAA depth input"),
            layout: depth_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&depth),
            }],
        });
        FrameTargets {
            msaa,
            opaque,
            hdr,
            depth,
            resolved_depth,
            volume,
            opaque_inputs,
            water_inputs,
            display_inputs,
            volume_inputs,
            depth_inputs,
        }
    }
    fn depth(d: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
        d.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
    }
    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 || (w == self.config.width && h == self.config.height) {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.targets = Self::targets(&self.device, w, h, &self.post_layout, &self.depth_layout);
    }
    pub fn mesh_buffer(&mut self, data: &[u8], vertices: bool) {
        let b = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("World mesh"),
                contents: data,
                usage: if vertices {
                    wgpu::BufferUsages::VERTEX
                } else {
                    wgpu::BufferUsages::INDEX
                },
            });
        if vertices {
            self.vertex = Some(b)
        } else {
            self.index = Some(b)
        }
    }
    pub fn texture(&mut self, width: u32, height: u32, srgb: bool, sizes: &[usize], raw: &[u8]) {
        let t = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Baked material"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: sizes.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if srgb {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let (mut w, mut h, mut offset) = (width, height, 0);
        for (mip, &size) in sizes.iter().enumerate() {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &t,
                    mip_level: mip as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &raw[offset..offset + size],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            offset += size;
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        self.textures.push(t.create_view(&Default::default()));
    }
    pub fn materials(&mut self, materials: &[Material], geography: [u32; 4]) {
        for mat in materials {
            let uniform = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Material"),
                    contents: bytemuck::bytes_of(mat),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let ids = [
                mat.textures[0],
                mat.textures[1],
                mat.textures[2],
                mat.textures[3],
                mat.extra[0],
                mat.extra[1],
                if mat.settings[0] == 4. {
                    mat.settings[3] as u32
                } else {
                    geography[2]
                },
                geography[1],
            ];
            let mut entries = vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ];
            for (i, id) in ids.iter().enumerate() {
                entries.push(wgpu::BindGroupEntry {
                    binding: i as u32 + 2,
                    resource: wgpu::BindingResource::TextureView(&self.textures[*id as usize]),
                });
            }
            self.materials
                .push(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.material_layout,
                    entries: &entries,
                }));
        }
    }
    /// Bake terrain-relative transport once; sun/moon relighting uses the stored directional coefficients.
    pub async fn bake_lighting(
        &mut self,
        environment: [f32; 4],
        geography: [u32; 4],
        progress: impl Fn(u32, u32),
    ) -> Result<(), String> {
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let size = environment[2] as u32;
        let layers = environment[3] as u32;
        let probes = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Full-map directional light transfer"),
            size: size as u64 * size as u64 * layers as u64 * 80,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Geography clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shadow_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Filtered shadows"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        self.lighting = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("World light transfer"),
            layout: &self.lighting_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: probes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.shadow_views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.shadow_views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        &self.textures[environment[1] as usize],
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(
                        &self.textures[geography[2] as usize],
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(
                        &self.textures[geography[0] as usize],
                    ),
                },
            ],
        }));
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Voxel transport bake"),
                source: wgpu::ShaderSource::Wgsl(include_str!("web_transport.wgsl").into()),
            });
        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Voxel transport"),
                layout: None,
                module: &shader,
                entry_point: Some("bake"),
                compilation_options: Default::default(),
                cache: None,
            });
        let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Bake slice"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &self.textures[environment[0] as usize],
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &self.textures[environment[1] as usize],
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: probes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        if let Some(e) = self.device.pop_error_scope().await {
            return Err(e.to_string());
        }
        // Yield between slices: do not queue the whole expensive bake in one submission.
        for slice in 0..layers {
            progress(slice + 1, layers);
            self.queue.write_buffer(
                &uniform,
                0,
                bytemuck::cast_slice(&[size, layers, slice, 0u32]),
            );
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Bake slice"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(size.div_ceil(8), size.div_ceil(8), 1);
            }
            self.queue.submit(Some(encoder.finish()));
            let (tx, rx) = futures_channel::oneshot::channel();
            self.queue.on_submitted_work_done(move || {
                let _ = tx.send(());
            });
            let _ = rx.await;
        }
        Ok(())
    }
    fn draw_list<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, draws: &[Draw]) {
        for draw in draws {
            pass.set_bind_group(1, &self.materials[draw.material as usize], &[]);
            let (first, base) = if let Some(page) = self.pages.get(&draw.mesh.vertex_offset) {
                pass.set_vertex_buffer(0, page.vertex.slice(..));
                pass.set_index_buffer(page.index.slice(..), wgpu::IndexFormat::Uint32);
                (draw.mesh.first_index - page.first, 0)
            } else {
                pass.set_vertex_buffer(0, self.vertex.as_ref().unwrap().slice(..));
                pass.set_index_buffer(
                    self.index.as_ref().unwrap().slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                (draw.mesh.first_index, draw.mesh.vertex_offset)
            };
            pass.draw_indexed(
                first..first + draw.mesh.index_count,
                base,
                draw.first_instance..draw.first_instance + draw.instances,
            );
        }
    }
    pub fn upload(&mut self, range: MeshRange, mesh: &MeshData) {
        if mesh.indices.is_empty() {
            self.pages.remove(&range.vertex_offset);
            return;
        }
        let vertex = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Grass page"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Grass indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        self.pages.insert(
            range.vertex_offset,
            Page {
                vertex,
                index,
                first: range.first_index,
            },
        );
    }
    pub fn render(
        &mut self,
        g: &Globals,
        instances: &[Instance],
        lists: DrawLists<'_>,
    ) -> Result<(), String> {
        let output = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        self.queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(g));
        self.queue
            .write_buffer(&self.instances, 0, bytemuck::cast_slice(instances));
        let target = output.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format.add_srgb_suffix()),
            ..Default::default()
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        for (i, list) in [lists.shadows, lists.far_shadows].iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Cascaded shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_views[i],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shadow_pipelines[i]);
            pass.set_bind_group(0, &self.global, &[]);
            pass.set_vertex_buffer(1, self.instances.slice(..));
            self.draw_list(&mut pass, list);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Opaque HDR MSAA"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.msaa,
                    depth_slice: None,
                    resolve_target: Some(&self.targets.opaque),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.global, &[]);
            pass.set_pipeline(&self.sky);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(
                2,
                self.lighting.as_ref().ok_or("Light bake not ready")?,
                &[],
            );
            pass.set_bind_group(3, &self.targets.opaque_inputs, &[]);
            pass.set_vertex_buffer(1, self.instances.slice(..));
            self.draw_list(&mut pass, lists.opaque);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Resolve depth for water"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.resolved_depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.depth_resolve);
            pass.set_bind_group(0, &self.targets.depth_inputs, &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("River reflection and refraction"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.msaa,
                    depth_slice: None,
                    resolve_target: Some(&self.targets.hdr),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.water);
            pass.set_bind_group(0, &self.global, &[]);
            pass.set_bind_group(2, self.lighting.as_ref().unwrap(), &[]);
            pass.set_bind_group(3, &self.targets.water_inputs, &[]);
            pass.set_vertex_buffer(1, self.instances.slice(..));
            self.draw_list(&mut pass, lists.water);
        }
        for (label, view, pipeline, inputs) in [
            (
                "Local light shafts",
                &self.targets.volume,
                &self.volume_pipeline,
                &self.targets.volume_inputs,
            ),
            (
                "HDR display",
                &target,
                &self.display_pipeline,
                &self.targets.display_inputs,
            ),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.global, &[]);
            pass.set_bind_group(1, &self.materials[0], &[]);
            pass.set_bind_group(2, self.lighting.as_ref().unwrap(), &[]);
            pass.set_bind_group(3, inputs, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}
