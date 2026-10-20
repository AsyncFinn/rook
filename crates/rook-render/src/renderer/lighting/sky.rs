//! The sky's reflection cube (`shaders/sky_cube.wgsl`): what reflective
//! surfaces show, built again whenever the sky changes. Studio reads it at
//! level 5 × a surface's remapped roughness, and Reflectance at level 0.

use wesl_core::include_wesl;

use super::*;

/// Texels along each face of the cube (Inference: Studio's size isn't
/// known; its reflections of the default sky look about this sharp).
const CUBE_SIZE: u32 = 256;

/// The reflection cube's levels: level m is roughness m / 5.
const CUBE_LEVELS: u32 = 6;

/// The format both cubes are stored in: light units above 1 (bright sky
/// images squared stay at most 1, but plain skies and later the sun don't).
const CUBE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// What the sky shader reads (`Params` in `shaders/sky_cube.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    faces: [[f32; 4]; 18],
    top: [f32; 4],
    horizon: [f32; 4],
    flags: [u32; 4],
}

/// The sky a cube was built from.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Built {
    images: Option<[ImageHash; 6]>,
    top: [f32; 3],
    horizon: [f32; 3],
}

#[derive(Debug)]
pub(crate) struct SkyCube {
    capture_layout: wgpu::BindGroupLayout,
    filter_layout: wgpu::BindGroupLayout,
    capture: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
    prefilter: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
    /// Stands in for the skybox's images while there are none.
    blank: wgpu::TextureView,
    /// The sky drawn onto a cube and its box mips, one level fewer than
    /// the reflection cube (its level m reads the sky's m - 1).
    sky: wgpu::Texture,
    cube: wgpu::Texture,
    /// The reflection cube, as the main pass samples it.
    pub(crate) view: wgpu::TextureView,
    built: Option<Built>,
}

impl SkyCube {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let texture_entry = |binding, dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: fragment,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: dimension,
                multisampled: false,
            },
            count: None,
        };
        let sampler_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: fragment,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let mut capture_entries = vec![
            sampler_entry,
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: fragment,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        capture_entries
            .extend((2..8).map(|binding| texture_entry(binding, wgpu::TextureViewDimension::D2)));
        let capture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook sky capture"),
            entries: &capture_entries,
        });
        let filter_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook sky filter"),
            entries: &[
                sampler_entry,
                texture_entry(8, wgpu::TextureViewDimension::Cube),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook sky cube"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("sky_cube").into()),
        });
        let pipeline = |label, layout: &wgpu::BindGroupLayout, entry| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_cube"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(CUBE_FORMAT.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let capture = pipeline("rook sky capture", &capture_layout, "fs_capture");
        let downsample = pipeline("rook sky mips", &filter_layout, "fs_downsample");
        let prefilter = pipeline("rook sky prefilter", &filter_layout, "fs_prefilter");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rook sky cube"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rook sky cube"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blank = device
            .create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("rook sky blank"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                Default::default(),
                &[0, 0, 0, 255],
            )
            .create_view(&Default::default());
        let cube_texture = |label, levels, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: CUBE_SIZE,
                    height: CUBE_SIZE,
                    depth_or_array_layers: 6,
                },
                mip_level_count: levels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: CUBE_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | usage,
                view_formats: &[],
            })
        };
        let sky = cube_texture("rook sky", CUBE_LEVELS - 1, wgpu::TextureUsages::COPY_SRC);
        let cube = cube_texture(
            "rook sky reflections",
            CUBE_LEVELS,
            wgpu::TextureUsages::COPY_DST,
        );
        let view = cube_view(&cube, 0, CUBE_LEVELS);
        Self {
            capture_layout,
            filter_layout,
            capture,
            downsample,
            prefilter,
            sampler,
            params,
            blank,
            sky,
            cube,
            view,
            built: None,
        }
    }

    /// Build the cube again if the sky has changed since it was built:
    /// from the skybox's `images` (as stored, with their hashes) once all
    /// six are up, else from the plain sky's colours (sRGB).
    pub(crate) fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        images: Option<([ImageHash; 6], [&wgpu::TextureView; 6])>,
        top: [f32; 3],
        horizon: [f32; 3],
    ) {
        // With images, the plain colours (which change with the time of
        // day) don't count.
        let plain = |c: [f32; 3]| if images.is_some() { [0.0; 3] } else { c };
        let built = Built {
            images: images.map(|(hashes, _)| hashes),
            top: plain(top),
            horizon: plain(horizon),
        };
        if self.built == Some(built) {
            return;
        }
        self.built = Some(built);
        let mut faces = [[0.0; 4]; 18];
        for (i, (dir, right, up)) in SKYBOX_FACES.iter().enumerate() {
            faces[i * 3] = dir.extend(0.0).to_array();
            faces[i * 3 + 1] = right.extend(0.0).to_array();
            faces[i * 3 + 2] = up.extend(0.0).to_array();
        }
        let params = Params {
            faces,
            top: [top[0], top[1], top[2], 0.0],
            horizon: [horizon[0], horizon[1], horizon[2], 0.0],
            flags: [u32::from(images.is_some()), CUBE_SIZE, 0, 0],
        };
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
        let views = images.map_or([&self.blank; 6], |(_, views)| views);
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: self.params.as_entire_binding(),
            },
        ];
        entries.extend(
            views
                .iter()
                .enumerate()
                .map(|(i, view)| wgpu::BindGroupEntry {
                    binding: 2 + i as u32,
                    resource: wgpu::BindingResource::TextureView(view),
                }),
        );
        let capture = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook sky capture"),
            layout: &self.capture_layout,
            entries: &entries,
        });
        let filter = |source: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rook sky filter"),
                layout: &self.filter_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                ],
            })
        };

        draw_levels(encoder, &self.sky, 0, &self.capture, &capture);
        for level in 1..CUBE_LEVELS - 1 {
            let before = filter(&cube_view(&self.sky, level - 1, 1));
            draw_levels(encoder, &self.sky, level, &self.downsample, &before);
        }
        encoder.copy_texture_to_texture(
            self.sky.as_image_copy(),
            self.cube.as_image_copy(),
            wgpu::Extent3d {
                width: CUBE_SIZE,
                height: CUBE_SIZE,
                depth_or_array_layers: 6,
            },
        );
        let sky = filter(&cube_view(&self.sky, 0, CUBE_LEVELS - 1));
        for level in 1..CUBE_LEVELS {
            draw_levels(encoder, &self.cube, level, &self.prefilter, &sky);
        }
    }
}

/// `texture`'s levels from `first`, `count` of them, as a cube.
fn cube_view(texture: &wgpu::Texture, first: u32, count: u32) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        base_mip_level: first,
        mip_level_count: Some(count),
        ..Default::default()
    })
}

/// Draw each face of `texture`'s `level` with `pipeline` (the shader
/// takes the face and level from the instance, level × 6 + face).
fn draw_levels(
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    level: u32,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
) {
    for face in 0..6 {
        let target = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: level,
            mip_level_count: Some(1),
            base_array_layer: face,
            array_layer_count: Some(1),
            ..Default::default()
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rook sky cube"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        let at = level * 6 + face;
        pass.draw(0..3, at..at + 1);
    }
}

/// Studio's split-sum table (the install's `textures/brdfLUT.dds`):
/// `size` × `size` pairs of half floats, A and B, by remapped roughness
/// across and the cosine to the camera down.
pub(crate) fn brdf_table(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: u32,
    rg16: &[u8],
) -> wgpu::TextureView {
    table_view(device, queue, size, wgpu::TextureFormat::Rg16Float, rg16)
}

/// Rook's stand-in for Studio's table without an install: the share of
/// the reflection that's white, B / (A + B), as (1 - roughness)^2 ×
/// (1 - cosine)^4 (fitted to the install's table, within 0.11 of it).
pub(crate) fn rooks_brdf_table(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    const SIZE: u32 = 32;
    let at = |i: u32| (i as f32 + 0.5) / SIZE as f32;
    let texels: Vec<u8> = (0..SIZE)
        .flat_map(|y| (0..SIZE).map(move |x| (at(x), at(y))))
        .flat_map(|(roughness, cosine)| {
            let white = (1.0 - roughness).powi(2) * (1.0 - cosine).powi(4);
            let byte = |v: f32| (v * 255.0).round() as u8;
            [byte(1.0 - white), byte(white), 0, 255]
        })
        .collect();
    table_view(
        device,
        queue,
        SIZE,
        wgpu::TextureFormat::Rgba8Unorm,
        &texels,
    )
}

fn table_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: u32,
    format: wgpu::TextureFormat,
    texels: &[u8],
) -> wgpu::TextureView {
    device
        .create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("rook brdf table"),
                size: wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            Default::default(),
            texels,
        )
        .create_view(&Default::default())
}
