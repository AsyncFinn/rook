//! The GUI pass: the GUI's rectangles drawn in order into the GUI layer,
//! premultiplied and in sRGB, which the tone pass lays over the frame, so
//! GUI blends in sRGB as Roblox's does (docs/behavior/gui/layout.md#drawing).
//! [`world`] draws the world GUI's rectangles into the world. Text quads
//! draw through their glyph's coverage in the glyph atlas, image quads
//! through their image ([`images`]).

mod atlas;
mod images;
pub(crate) mod world;

use rook_types::{Paint, WorldCanvas, WorldPlace};
use wesl_core::include_wesl;

use super::*;
use atlas::Atlas;
use images::{GuiImages, ImageKey, Run, runs};
use pipelines::BindLayouts;
use world::{WorldGui, WorldRuns};

/// A list of quads as the shader reads them, each with its image.
type Packed = Vec<(GpuQuad, Option<ImageKey>)>;

/// The quads' vertex attributes, in [`GpuQuad`]'s order.
const QUAD_ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
    0 => Float32x2, 1 => Float32x2, 2 => Float32x2,
    3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32,
];

/// The GUI layer's format: 8-bit sRGB values, blended as stored.
pub(crate) const GUI_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A [`GuiQuad`] as the shader reads it.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(super) struct GpuQuad {
    center: [f32; 2],
    half_size: [f32; 2],
    turn: [f32; 2],
    color: [f32; 4],
    clip: [f32; 4],
    /// The glyph's rectangle in the atlas, or the image's part, 0 to 1.
    uv: [f32; 4],
    /// 0 fills the rectangle, 1 draws through a coverage mask, 2 through a
    /// distance field, 3 through an image (`shaders/gui_paint.wgsl`).
    mode: f32,
}

impl GpuQuad {
    /// `q`, its glyph or image at `uv`.
    fn new(q: &GuiQuad, uv: [f32; 4]) -> Self {
        let a = q.color[3].clamp(0.0, 1.0);
        let (sin, cos) = q.rotation.sin_cos();
        Self {
            center: q.center.to_array(),
            half_size: (q.size * 0.5).to_array(),
            turn: [sin, cos],
            color: [q.color[0] * a, q.color[1] * a, q.color[2] * a, a],
            clip: q.clip,
            uv,
            mode: match &q.paint {
                Paint::Solid => 0.0,
                Paint::Glyph(mask) if mask.distance => 2.0,
                Paint::Glyph(_) => 1.0,
                Paint::Image(_) => 3.0,
            },
        }
    }
}

impl GpuQuad {
    /// The quads as instances, one per quad.
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuQuad>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &QUAD_ATTRIBUTES,
        }
    }
}

/// What the GUI pass draws into the screen's GUI layer.
#[derive(Debug)]
struct Target {
    /// The target's size.
    size: wgpu::Buffer,
    group: wgpu::BindGroup,
    /// The quads, and how many there are.
    quads: Option<(wgpu::Buffer, u32)>,
    runs: Vec<Run>,
}

impl Target {
    fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        glyphs: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let size = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rook gui target size"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook gui"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: size.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(glyphs),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        Self {
            size,
            group,
            quads: None,
            runs: Vec::new(),
        }
    }

    /// Draw `packed` from now on.
    fn set(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, packed: &Packed) {
        self.runs = runs(packed.iter().map(|(_, image)| *image));
        if packed.is_empty() {
            self.quads = None;
            return;
        }
        let gpu: Vec<GpuQuad> = packed.iter().map(|(q, _)| *q).collect();
        let bytes: &[u8] = bytemuck::cast_slice(&gpu);
        let reuse = matches!(&self.quads, Some((b, _)) if b.size() >= bytes.len() as u64);
        if !reuse {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rook gui quads"),
                size: (bytes.len() as u64).next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.quads = Some((buffer, 0));
        }
        if let Some((buffer, count)) = &mut self.quads {
            queue.write_buffer(buffer, 0, bytes);
            *count = gpu.len() as u32;
        }
    }
}

#[derive(Debug)]
pub(crate) struct GuiPass {
    pipeline: wgpu::RenderPipeline,
    screen: Target,
    world: WorldGui,
    atlas: Atlas,
    images: GuiImages,
}

impl GuiPass {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layouts: BindLayouts<'_>,
        table: bool,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook gui"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let atlas = Atlas::new(device);
        let atlas_view = atlas.texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rook gui glyphs"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook gui"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("gui").into()),
        });
        let images = GuiImages::new(device, queue);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rook gui"),
            bind_group_layouts: &[Some(&layout), Some(&images.layout)],
            immediate_size: 0,
        });
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rook gui"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_gui"),
                compilation_options: Default::default(),
                buffers: &[Some(GpuQuad::layout())],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_gui"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: GUI_FORMAT,
                    blend: Some(wgpu::BlendState {
                        color: premultiplied,
                        alpha: premultiplied,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            screen: Target::new(device, &layout, &atlas_view, &sampler),
            world: WorldGui::new(device, layouts, table, atlas_view),
            atlas,
            images,
        }
    }

    /// Draw `screen` on the screen from now on, bottom first, and the
    /// world GUI's `canvases`.
    pub(crate) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &[GuiQuad],
        canvases: &[WorldCanvas],
    ) {
        let lists: Vec<&[GuiQuad]> = std::iter::once(screen)
            .chain(canvases.iter().map(|c| &c.quads[..]))
            .collect();
        let packed = self.pack(queue, &lists);
        self.screen.set(device, queue, &packed[0]);
        self.world.set(device, canvases, &packed[1..]);
    }

    /// Where each world GUI canvas is from now on.
    pub(crate) fn place(&mut self, places: Vec<Option<WorldPlace>>) {
        self.world.place(places);
    }

    /// Each of `lists` as the shader reads them, their glyphs in the
    /// atlas. If the atlas fills and starts again partway, every glyph is
    /// placed again once, so none points at a mask that was written over.
    fn pack(&mut self, queue: &wgpu::Queue, lists: &[&[GuiQuad]]) -> Vec<Packed> {
        let scale = 1.0 / atlas::SIZE as f32;
        let once = |atlas: &mut Atlas| -> Vec<Packed> {
            (lists.iter())
                .map(|quads| {
                    quads
                        .iter()
                        .filter_map(|q| {
                            let (uv, image) = match &q.paint {
                                Paint::Solid => ([0.0; 4], None),
                                Paint::Glyph(mask) => {
                                    (atlas.place(queue, mask)?.map(|t| t * scale), None)
                                }
                                Paint::Image(image) => (image.uv, Some(ImageKey::of(image))),
                            };
                            Some((GpuQuad::new(q, uv), image))
                        })
                        .collect()
                })
                .collect()
        };
        let resets = self.atlas.resets;
        let gpu = once(&mut self.atlas);
        if self.atlas.resets == resets {
            return gpu;
        }
        once(&mut self.atlas)
    }

    /// Bind the images the GUI draws that are on the GPU now (`cache`),
    /// and upload the world GUI's canvases `camera` sees this frame, in a
    /// `height`-pixel window.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &GpuCache<ImageHash, Image>,
        camera: &Camera,
        height: u32,
    ) -> WorldRuns {
        self.images.bind(device, &self.screen.runs, cache);
        let images = (&self.images, cache);
        self.world.upload(device, queue, images, camera, height)
    }

    /// Lay the world GUI's canvases in the world (last in the main pass).
    pub(crate) fn record_world(&self, pass: &mut wgpu::RenderPass<'_>, runs: &WorldRuns) {
        self.world.record(pass, runs);
    }

    /// Draw the GUI layer into `target`, if there's any GUI. Whether there
    /// was.
    pub(crate) fn record(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
    ) -> bool {
        self.draw(queue, encoder, &self.screen, target, size)
    }

    /// Draw `quads` into `view`, cleared first. Whether there were any.
    fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        quads: &Target,
        view: &wgpu::TextureView,
        (width, height): (u32, u32),
    ) -> bool {
        let Some((buffer, count)) = &quads.quads else {
            return false;
        };
        let size = [width as f32, height as f32, 0.0, 0.0];
        queue.write_buffer(&quads.size, 0, bytemuck::bytes_of(&size));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rook gui"),
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
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &quads.group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        // Quads whose image isn't up yet draw nothing.
        for run in quads.runs.iter().filter(|r| r.quads.end <= *count) {
            if let Some(group) = self.images.group(run.image) {
                pass.set_bind_group(1, group, &[]);
                pass.draw(0..6, run.quads.clone());
            }
        }
        true
    }
}
