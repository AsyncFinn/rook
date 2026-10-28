//! The render pipelines and the shader they share.

use wesl_core::include_wesl;

use super::*;

/// The shaders (`shaders/`, linked by the build script): parts and
/// terrain, the skybox, and particles, for how draws reach their images
/// (a table, or a set bound per draw).
fn shaders(table: bool) -> [&'static str; 3] {
    match table {
        true => [
            include_wesl!("parts_table"),
            include_wesl!("sky_table"),
            include_wesl!("particles_table"),
        ],
        false => [
            include_wesl!("parts"),
            include_wesl!("sky"),
            include_wesl!("particles"),
        ],
    }
}

/// The bind group layouts the part pipelines use, in group order.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BindLayouts<'a> {
    pub(crate) globals: &'a wgpu::BindGroupLayout,
    pub(crate) main: &'a wgpu::BindGroupLayout,
    pub(crate) image: &'a wgpu::BindGroupLayout,
    pub(crate) lights: &'a wgpu::BindGroupLayout,
}

/// The render pipelines, built once.
#[derive(Debug)]
pub(crate) struct Pipelines {
    pub(crate) opaque: wgpu::RenderPipeline,
    pub(crate) transparent: wgpu::RenderPipeline,
    /// Decals and Textures over what's drawn.
    pub(crate) layer: wgpu::RenderPipeline,
    /// See-through Glass's nearest depth, before its filter, so a union's
    /// overlapping shells filter once (Inference: Studio shows an
    /// extinguisher clearly through a Glass union with three layers, which
    /// Rook darkened to 3% when each filtered).
    pub(crate) glass_depth: wgpu::RenderPipeline,
    /// See-through Glass: what's behind times its filter, where the Glass
    /// is nearest.
    pub(crate) glass_filter: wgpu::RenderPipeline,
    /// See-through Glass's own surface, where it's nearest.
    pub(crate) glass_surface: wgpu::RenderPipeline,
    /// ForceField: its glow added to what's behind.
    pub(crate) additive: wgpu::RenderPipeline,
    /// Terrain seams' later layers: their share added to the first's, on
    /// the same surface (the same vertices, so the same depth).
    pub(crate) over: wgpu::RenderPipeline,
    /// Adornments always on top: blended over everything, untested.
    pub(crate) on_top: wgpu::RenderPipeline,
    /// Highlighted parts into the Highlight mask (`highlights.rs`): over
    /// everything, or only where seen.
    pub(crate) highlight_on_top: wgpu::RenderPipeline,
    pub(crate) highlight_occluded: wgpu::RenderPipeline,
    /// The skybox, behind everything.
    pub(crate) sky: wgpu::RenderPipeline,
    /// The plain sky color, before (or without) the skybox.
    pub(crate) plain_sky: wgpu::RenderPipeline,
    /// Depth only, from the sun, over the opaque batches.
    pub(crate) shadow: wgpu::RenderPipeline,
    /// Emitters' particles, over everything.
    pub(crate) particles: wgpu::RenderPipeline,
    /// Beams and Trails, before the particles.
    pub(crate) ribbons: wgpu::RenderPipeline,
}

impl Pipelines {
    /// Pipelines drawing into a `frame_format` frame (`tonemap.rs`).
    pub(crate) fn new(
        device: &wgpu::Device,
        frame_format: wgpu::TextureFormat,
        layouts: BindLayouts<'_>,
        table: bool,
    ) -> Self {
        let [parts, sky, particles] = shaders(table);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook parts"),
            source: wgpu::ShaderSource::Wgsl(parts.into()),
        });
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook sky"),
            source: wgpu::ShaderSource::Wgsl(sky.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rook parts"),
            bind_group_layouts: &[
                Some(layouts.globals),
                Some(layouts.main),
                Some(layouts.image),
                Some(layouts.lights),
            ],
            immediate_size: 0,
        });
        // The mesh pool's two vertex streams, then the instances.
        let position_attrs = wgpu::vertex_attr_array![0 => Float32x3];
        let vertex_attrs = wgpu::vertex_attr_array![
            1 => Float32x3, 2 => Float32x2, 3 => Float32x3, 4 => Uint32
        ];
        let instance_attrs = wgpu::vertex_attr_array![
            5 => Float32x3, 6 => Float32x3, 7 => Float32x3, 8 => Float32x3,
            9 => Float32x3, 10 => Float32x4, 11 => Float32x3, 12 => Float32x3,
            13 => Float32x4, 14 => Uint32x4, 15 => Float32x4
        ];
        let positions = wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &position_attrs,
        };
        let instances = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &instance_attrs,
        };
        let buffers = [
            Some(positions.clone()),
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<pool::VertexAttributes>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &vertex_attrs,
            }),
            Some(instances.clone()),
        ];
        // The shadow pass moves terrain's waves and blades as the main
        // pass does, so it reads their vertices' uv and face too.
        let shadow_buffers = buffers.clone();
        let depth_format = main_depth_format(device);
        // See-through Glass's filter and surface each draw a pixel once
        // per part: the first fragment there stamps the part's reference
        // (`Renderer::record` gives each Glass run its own) and ties with
        // it at the same depth fail. Without a stencil, ties draw twice,
        // in thin dark lines where a union's shells meet (observed on an
        // extinguisher box's cover).
        let once = if depth_format.has_stencil_aspect() {
            let face = wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::NotEqual,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Replace,
            };
            wgpu::StencilState {
                front: face,
                back: face,
                read_mask: 0xff,
                write_mask: 0xff,
            }
        } else {
            wgpu::StencilState::default()
        };
        let make = |label,
                    fragment,
                    blend: Option<wgpu::BlendState>,
                    depth_write: bool,
                    compare: wgpu::CompareFunction,
                    bias: wgpu::DepthBiasState,
                    stencil: wgpu::StencilState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth_format,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(compare),
                    stencil,
                    bias,
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: frame_format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let shadow_pipeline = shadow(device, layouts.globals, &shader, &shadow_buffers);
        let highlight = |on_top| {
            super::highlights::pipeline(device, layouts.globals, &shader, &buffers, on_top)
        };
        let (highlight_on_top, highlight_occluded) = (highlight(true), highlight(false));
        // Reverse Z: nearer is greater.
        let opaque_pipeline = make(
            "rook opaque",
            "fs_main",
            None,
            true,
            wgpu::CompareFunction::Greater,
            Default::default(),
            Default::default(),
        );
        let transparent_pipeline = make(
            "rook transparent",
            "fs_main",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            wgpu::CompareFunction::Greater,
            Default::default(),
            Default::default(),
        );
        // Depth only: the color stays as it is.
        let keep = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let glass_depth = make(
            "rook glass depth",
            "fs_glass_filter",
            Some(wgpu::BlendState {
                color: keep,
                alpha: keep,
            }),
            true,
            wgpu::CompareFunction::Greater,
            Default::default(),
            Default::default(),
        );
        let glass_filter = make(
            "rook glass filter",
            "fs_glass_filter",
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::Src,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
            false,
            wgpu::CompareFunction::Equal,
            Default::default(),
            once.clone(),
        );
        let glass_surface = make(
            "rook glass surface",
            "fs_main",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            wgpu::CompareFunction::GreaterEqual,
            Default::default(),
            once,
        );
        let additive = make(
            "rook additive",
            "fs_force_field",
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
            false,
            wgpu::CompareFunction::Greater,
            Default::default(),
            Default::default(),
        );
        let over = make(
            "rook terrain seams",
            "fs_main",
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            false,
            wgpu::CompareFunction::GreaterEqual,
            Default::default(),
            Default::default(),
        );
        let on_top = make(
            "rook on top",
            "fs_main",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            wgpu::CompareFunction::Always,
            Default::default(),
            Default::default(),
        );
        // Images lie exactly on their part's surface: the same vertices
        // through the same (invariant) maths give the same depth.
        let layer_pipeline = make(
            "rook images",
            "fs_layer",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            wgpu::CompareFunction::GreaterEqual,
            // Towards the camera (greater in reverse Z), so an image also
            // wins over another part's face lying in the same plane, as in
            // Roblox, instead of fighting it.
            wgpu::DepthBiasState {
                constant: 4,
                slope_scale: 1.0,
                clamp: 0.0,
            },
            Default::default(),
        );

        // The skybox: one full-screen triangle per image, each keeping the
        // pixels whose direction its face covers; nothing written to depth,
        // so everything draws over it.
        let sky_pipeline = |fragment| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("rook sky"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &sky_shader,
                    entry_point: Some("vs_sky"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth_format,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &sky_shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: frame_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let particle_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook particles"),
            source: wgpu::ShaderSource::Wgsl(particles.into()),
        });
        let particles = super::effects::particles::pipeline(
            device,
            &pipeline_layout,
            &particle_shader,
            frame_format,
            depth_format,
        );
        let ribbons = super::effects::ribbons::pipeline(
            device,
            &pipeline_layout,
            &particle_shader,
            frame_format,
            depth_format,
        );
        Self {
            particles,
            ribbons,
            opaque: opaque_pipeline,
            transparent: transparent_pipeline,
            layer: layer_pipeline,
            glass_depth,
            glass_filter,
            glass_surface,
            additive,
            over,
            on_top,
            highlight_on_top,
            highlight_occluded,
            sky: sky_pipeline("fs_sky"),
            plain_sky: sky_pipeline("fs_plain_sky"),
            shadow: shadow_pipeline,
        }
    }
}

/// Depth only, from the sun: the shadow map.
fn shadow(
    device: &wgpu::Device,
    globals: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
) -> wgpu::RenderPipeline {
    // The shadow pass writes the map, so it can't bind it.
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rook shadow map"),
        bind_group_layouts: &[Some(globals)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("rook shadow map"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_shadow"),
            compilation_options: Default::default(),
            buffers,
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Greater),
            stencil: Default::default(),
            // Pushes casters away from the sun (smaller in reverse Z)
            // where they slope across its view, against acne on faces
            // the sun grazes; the normal offset handles the rest.
            bias: wgpu::DepthBiasState {
                constant: 0,
                slope_scale: -1.5,
                clamp: 0.0,
            },
        }),
        multisample: Default::default(),
        fragment: None,
        multiview_mask: None,
        cache: None,
    })
}
