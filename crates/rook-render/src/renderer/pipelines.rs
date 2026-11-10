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

/// The pipelines that draw a part's own surface, in one culling. A
/// `DoubleSided` MeshPart takes them without culling; images stay on the
/// front (`docs/behavior/meshes/double-sided.md`).
#[derive(Debug)]
pub(crate) struct Surfaces {
    pub(crate) opaque: wgpu::RenderPipeline,
    pub(crate) transparent: wgpu::RenderPipeline,
    /// See-through Glass's nearest depth, before its filter, so a union's
    /// overlapping shells filter once (Inference: Studio shows an
    /// extinguisher clearly through a Glass union with three layers, which
    /// Rook darkened to 3% when each filtered).
    pub(crate) glass_depth: wgpu::RenderPipeline,
    /// See-through Glass's own surface, where it's nearest.
    pub(crate) glass_surface: wgpu::RenderPipeline,
    /// ForceField: its glow added to what's behind.
    pub(crate) additive: wgpu::RenderPipeline,
    /// Highlighted parts into the Highlight mask (`highlights.rs`): over
    /// everything, or only where seen.
    pub(crate) highlight_on_top: wgpu::RenderPipeline,
    pub(crate) highlight_occluded: wgpu::RenderPipeline,
    /// Depth only, from the sun, over the opaque batches.
    pub(crate) shadow: wgpu::RenderPipeline,
}

/// The render pipelines, built once.
#[derive(Debug)]
pub(crate) struct Pipelines {
    /// Parts' surfaces, back faces culled.
    pub(crate) front: Surfaces,
    /// `DoubleSided` meshes' surfaces, nothing culled.
    pub(crate) both: Surfaces,
    /// See-through Glass: what's behind times its filter, where the Glass
    /// is nearest. Front faces only, even on a `DoubleSided` mesh, whose
    /// back shows just the surface (`docs/behavior/meshes/double-sided.md`).
    pub(crate) glass_filter: wgpu::RenderPipeline,
    /// Terrain water's depth alone, before its color (`water`).
    pub(crate) water_depth: wgpu::RenderPipeline,
    /// Terrain water: transparent, not culled, drawn only where its depth
    /// pass left it nearest.
    pub(crate) water: wgpu::RenderPipeline,
    /// Decals and Textures over what's drawn.
    pub(crate) layer: wgpu::RenderPipeline,
    /// Terrain seams' later layers: their share added to the first's, on
    /// the same surface (the same vertices, so the same depth).
    pub(crate) over: wgpu::RenderPipeline,
    /// Adornments always on top: blended over everything, untested.
    pub(crate) on_top: wgpu::RenderPipeline,
    /// The skybox, behind everything.
    pub(crate) sky: wgpu::RenderPipeline,
    /// The plain sky color, before (or without) the skybox.
    pub(crate) plain_sky: wgpu::RenderPipeline,
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
        let make_faces = |cull: Option<wgpu::Face>,
                          label,
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
                    cull_mode: cull,
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
        let highlight = |on_top, cull| {
            super::highlights::pipeline(device, layouts.globals, &shader, &buffers, on_top, cull)
        };
        let make = |label, fragment, blend, depth_write, compare, bias, stencil| {
            make_faces(
                Some(wgpu::Face::Back),
                label,
                fragment,
                blend,
                depth_write,
                compare,
                bias,
                stencil,
            )
        };
        // Depth only: the color stays as it is.
        let keep = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        // Terrain water, seen from either side: Studio draws its underside
        // (`docs/behavior/terrain/water.md#seen-from-below`). Its depth
        // goes first, before the other transparent parts, so those under it
        // are hidden as Studio hides them, and its color then lands only on
        // the nearest water surface: Studio's see-through water never shows
        // the water behind it (its far banks and sides).
        let water_depth = make_faces(
            None,
            "rook water depth",
            "fs_depth",
            Some(wgpu::BlendState {
                color: keep,
                alpha: keep,
            }),
            true,
            wgpu::CompareFunction::Greater,
            Default::default(),
            Default::default(),
        );
        let water_pipeline = make_faces(
            None,
            "rook water",
            "fs_main",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            wgpu::CompareFunction::GreaterEqual,
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
        let surfaces = |cull| Surfaces {
            // Reverse Z: nearer is greater.
            opaque: make_faces(
                cull,
                "rook opaque",
                "fs_main",
                None,
                true,
                wgpu::CompareFunction::Greater,
                Default::default(),
                Default::default(),
            ),
            transparent: make_faces(
                cull,
                "rook transparent",
                "fs_main",
                Some(wgpu::BlendState::ALPHA_BLENDING),
                false,
                wgpu::CompareFunction::Greater,
                Default::default(),
                Default::default(),
            ),
            glass_depth: make_faces(
                cull,
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
            ),
            glass_surface: make_faces(
                cull,
                "rook glass surface",
                "fs_main",
                Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                false,
                wgpu::CompareFunction::GreaterEqual,
                Default::default(),
                once.clone(),
            ),
            additive: make_faces(
                cull,
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
            ),
            highlight_on_top: highlight(true, cull),
            highlight_occluded: highlight(false, cull),
            shadow: shadow(device, layouts.globals, &shader, &shadow_buffers, cull),
        };
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
            // Roblox, instead of fighting it. Two faces in one plane differ
            // by rounding: a few depth steps, and on a slope at most the
            // depth across 1/128 pixel (each face's corners snap to 1/256
            // pixel). A slope term of a whole pixel lifted a floor's
            // Texture through a grating 0.05 studs above it once the floor
            // was seen at a low angle.
            wgpu::DepthBiasState {
                constant: 4,
                slope_scale: 1.0 / 32.0,
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
            front: surfaces(Some(wgpu::Face::Back)),
            both: surfaces(None),
            glass_filter,
            water_depth,
            water: water_pipeline,
            layer: layer_pipeline,
            over,
            on_top,
            sky: sky_pipeline("fs_sky"),
            plain_sky: sky_pipeline("fs_plain_sky"),
        }
    }
}

/// Depth only, from the sun: the shadow map.
fn shadow(
    device: &wgpu::Device,
    globals: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    cull: Option<wgpu::Face>,
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
            cull_mode: cull,
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
