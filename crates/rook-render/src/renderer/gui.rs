//! The GUI pass: the GUI's rectangles drawn in order into the GUI layer,
//! premultiplied and in sRGB, which the tone pass lays over the frame, so
//! GUI blends in sRGB as Roblox's does (docs/behavior/gui/layout.md#drawing).
//! [`world`] draws the world GUI's rectangles into the world. Text quads
//! draw through their glyph's coverage in the glyph atlas, image quads
//! through their image ([`images`]), group quads through their group's
//! texture, drawn first ([`groups`]); gradients read their ramp
//! ([`ramps`]).

mod atlas;
mod groups;
mod images;
mod ramps;
mod screen;
pub(crate) mod world;

use std::sync::Arc;

use rook_types::{GuiGroup, Paint, WorldCanvas, WorldPlace};
use wesl_core::include_wesl;

use super::*;
use atlas::Atlas;
use groups::Groups;
use images::{GuiImages, ImageKey, Run, runs};
use pipelines::BindLayouts;
use ramps::Ramps;
use screen::ScreenPieces;
use world::{WorldDraw, WorldGui, WorldRuns};

/// A list of quads as the shader reads them, each with its image.
type Packed = Vec<(GpuQuad, Option<ImageKey>)>;

/// The groups a list of quads holds, inner ones first.
type Held = Vec<Arc<GuiGroup>>;

/// The quads' vertex attributes, in [`GpuQuad`]'s order.
const QUAD_ATTRIBUTES: [wgpu::VertexAttribute; 11] = wgpu::vertex_attr_array![
    0 => Float32x2, 1 => Float32x2, 2 => Float32x2,
    3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32,
    7 => Float32x4, 8 => Float32x4, 9 => Float32x4, 10 => Float32x4,
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
    /// distance field, 3 through an image, 4 through a group's texture
    /// (`shaders/gui_paint.wgsl`).
    mode: f32,
    /// The shape's center and half size (0: none).
    shape_rect: [f32; 4],
    /// The shape's corner radii, as [`rook_types::GuiShape`]'s.
    radii: [f32; 4],
    /// The shape's band and cut corners (1), and the gradient's ramp row
    /// (-1: none).
    shape: [f32; 4],
    /// Where the gradient is 0.5, and its change per pixel.
    gradient: [f32; 4],
}

impl GpuQuad {
    /// `q`, its glyph or image at `uv`, its gradient's ramp on row `ramp`.
    fn new(q: &GuiQuad, uv: [f32; 4], ramp: Option<u32>) -> Self {
        let a = q.color[3].clamp(0.0, 1.0);
        let (sin, cos) = q.rotation.sin_cos();
        let (shape_rect, radii, [ring, bevel]) = match &q.shape {
            Some(s) => (
                [s.center.x, s.center.y, s.size.x * 0.5, s.size.y * 0.5],
                s.radii,
                [s.ring, if s.bevel { 1.0 } else { 0.0 }],
            ),
            None => ([0.0; 4], [0.0; 4], [0.0; 2]),
        };
        let gradient = (q.gradient.as_ref())
            .map_or([0.0; 4], |g| [g.origin.x, g.origin.y, g.step.x, g.step.y]);
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
                Paint::Group(_) => 4.0,
                Paint::Outline(_, grow) => 5.0 + grow,
            },
            shape_rect,
            radii,
            shape: [ring, bevel, ramp.map_or(-1.0, |r| r as f32), 0.0],
            gradient,
        }
    }

    /// The quad as drawn `k` times bigger with its origin at `corner`
    /// (a canvas's quads laid on the screen).
    fn moved(self, corner: glam::Vec2, k: f32) -> Self {
        let at = |x: f32, y: f32| [corner.x + x * k, corner.y + y * k];
        let [l, t] = at(self.clip[0], self.clip[1]);
        let [r, b] = at(self.clip[2], self.clip[3]);
        let [sx, sy] = at(self.shape_rect[0], self.shape_rect[1]);
        let [gx, gy] = at(self.gradient[0], self.gradient[1]);
        Self {
            center: at(self.center[0], self.center[1]),
            half_size: self.half_size.map(|h| h * k),
            clip: [l, t, r, b],
            shape_rect: [sx, sy, self.shape_rect[2] * k, self.shape_rect[3] * k],
            radii: self.radii.map(|r| r * k),
            gradient: [gx, gy, self.gradient[2] / k, self.gradient[3] / k],
            ..self
        }
    }

    /// The quads as instances, one per quad.
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuQuad>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &QUAD_ATTRIBUTES,
        }
    }
}

/// What the GUI pass draws into a target: the screen's GUI layer or a
/// group's texture.
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
    fn new(device: &wgpu::Device, binds: Binds<'_>) -> Self {
        let size = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rook gui target size"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook gui"),
            layout: binds.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: size.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(binds.glyphs),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(binds.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(binds.ramps),
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

impl Target {
    /// Write `packed` over the quads from the `at`th on, which must be
    /// in the buffer already.
    fn write(&self, queue: &wgpu::Queue, at: u32, packed: &Packed) {
        let Some((buffer, _)) = &self.quads else {
            return;
        };
        if packed.is_empty() {
            return;
        }
        let gpu: Vec<GpuQuad> = packed.iter().map(|(q, _)| *q).collect();
        let offset = u64::from(at) * std::mem::size_of::<GpuQuad>() as u64;
        queue.write_buffer(buffer, offset, bytemuck::cast_slice(&gpu));
    }
}

/// What a [`Target`]'s bind group binds.
#[derive(Clone, Copy)]
struct Binds<'a> {
    layout: &'a wgpu::BindGroupLayout,
    glyphs: &'a wgpu::TextureView,
    ramps: &'a wgpu::TextureView,
    sampler: &'a wgpu::Sampler,
}

#[derive(Debug)]
pub(crate) struct GuiPass {
    pipeline: wgpu::RenderPipeline,
    /// The layout targets bind with, and the glyph atlas's and ramps'
    /// views and the sampler they bind.
    layout: wgpu::BindGroupLayout,
    glyphs: wgpu::TextureView,
    ramps_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    screen: Target,
    /// The untoned world canvases, under the screen's GUI.
    overlay: Target,
    /// The screen's pieces last given, and what was made of each.
    screen_pieces: ScreenPieces,
    groups: Groups,
    world: WorldGui,
    atlas: Atlas,
    ramps: Ramps,
    images: GuiImages,
}

impl GuiPass {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layouts: BindLayouts<'_>,
        table: bool,
    ) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
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
                texture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(3),
            ],
        });
        let atlas = Atlas::new(device);
        let glyphs = atlas.texture.create_view(&Default::default());
        let ramps = Ramps::new(device);
        let ramps_view = ramps.texture.create_view(&Default::default());
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
        let binds = Binds {
            layout: &layout,
            glyphs: &glyphs,
            ramps: &ramps_view,
            sampler: &sampler,
        };
        let screen = Target::new(device, binds);
        let overlay = Target::new(device, binds);
        let world = WorldGui::new(device, layouts, table, (&glyphs, &ramps_view));
        Self {
            pipeline,
            screen,
            overlay,
            screen_pieces: ScreenPieces::default(),
            groups: Groups::default(),
            world,
            layout,
            glyphs,
            ramps_view,
            sampler,
            atlas,
            ramps,
            images,
        }
    }

    /// Draw `screen`'s pieces on the screen from now on, bottom first, and
    /// the world GUI's `canvases`, with the groups they hold. Only pieces
    /// and canvases not given last time (another `Arc`) are packed and
    /// uploaded; those are returned.
    pub(crate) fn set<'a>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &'a Arc<[Arc<[GuiQuad]>]>,
        canvases: &'a [Arc<WorldCanvas>],
    ) -> Vec<&'a [GuiQuad]> {
        let new_screen = !self.screen_pieces.is(screen);
        let screen_kept = self.screen_pieces.kept(screen);
        let fresh_screen: Vec<&[GuiQuad]> = match new_screen {
            true => (screen.iter().zip(&screen_kept))
                .filter(|(_, k)| k.is_none())
                .map(|(p, _)| &p[..])
                .collect(),
            false => Vec::new(),
        };
        let fresh_pieces = fresh_screen.len();
        let kept = self.world.kept(canvases);
        let fresh: Vec<&[GuiQuad]> = (fresh_screen.into_iter())
            .chain(
                canvases
                    .iter()
                    .zip(&kept)
                    .filter(|(_, k)| k.is_none())
                    .map(|(c, _)| &c.quads[..]),
            )
            .collect();
        let fresh_held: Vec<Held> = fresh.iter().map(|q| groups::held(&[q])).collect();
        // Every group shown, inner ones first, each once.
        let mut shown: Vec<Arc<GuiGroup>> = Vec::new();
        let lists_held = (self.screen_pieces.held(&screen_kept))
            .chain(self.world.held(&kept))
            .chain(&fresh_held);
        for group in lists_held.flatten() {
            if !shown.iter().any(|g| g.key == group.key) {
                shown.push(group.clone());
            }
        }
        let unpacked: Vec<&Arc<GuiGroup>> = shown.iter().filter(|g| !self.groups.has(g)).collect();
        let lists: Vec<&[GuiQuad]> = (fresh.iter().copied())
            .chain(unpacked.iter().map(|g| &g.quads[..]))
            .collect();
        let (packed, reset) = self.pack(queue, &lists);
        // The masks or ramps what was kept points at were written over:
        // place everything again.
        if reset && lists.len() < screen.len() + canvases.len() + shown.len() {
            self.screen_pieces.forget();
            self.world.forget();
            self.groups.forget();
            return self.set(device, queue, screen, canvases);
        }
        let mut packed = packed.into_iter();
        let mut fresh_held = fresh_held.into_iter();
        if new_screen {
            let pieces: Vec<(Packed, Held)> = (&mut packed)
                .zip(&mut fresh_held)
                .take(fresh_pieces)
                .collect();
            (self.screen_pieces).set(
                device,
                queue,
                &mut self.screen,
                screen,
                &screen_kept,
                pieces,
            );
        }
        let world: Vec<(Packed, Held)> = (&mut packed).zip(&mut fresh_held).collect();
        self.world.set(device, canvases, kept, world);
        let binds = Binds {
            layout: &self.layout,
            glyphs: &self.glyphs,
            ramps: &self.ramps_view,
            sampler: &self.sampler,
        };
        let new_groups: Vec<(Arc<GuiGroup>, Packed)> =
            unpacked.into_iter().cloned().zip(packed).collect();
        let target = |d: &wgpu::Device| Target::new(d, binds);
        if let Some(views) = (self.groups).set(device, queue, &shown, &new_groups, target) {
            self.images.set_groups(views);
            self.world.forget_groups();
        }
        fresh
    }

    /// Where each world GUI canvas is from now on.
    pub(crate) fn place(&mut self, places: Vec<Option<WorldPlace>>) {
        self.world.place(places);
    }

    /// Each of `lists` as the shader reads them, their glyphs in the atlas
    /// and their gradients' ramps in theirs. If either fills and starts
    /// again partway, `lists` are placed again once, so none points at a
    /// mask or ramp that was written over, and whether that happened (so
    /// lists packed earlier may).
    fn pack(&mut self, queue: &wgpu::Queue, lists: &[&[GuiQuad]]) -> (Vec<Packed>, bool) {
        let scale = 1.0 / atlas::SIZE as f32;
        let once = |atlas: &mut Atlas, ramps: &mut Ramps| -> Vec<Packed> {
            (lists.iter())
                .map(|quads| {
                    quads
                        .iter()
                        .filter_map(|q| {
                            let (uv, image) = match &q.paint {
                                Paint::Solid => ([0.0; 4], None),
                                Paint::Glyph(mask) | Paint::Outline(mask, _) => {
                                    (atlas.place(queue, mask)?.map(|t| t * scale), None)
                                }
                                Paint::Image(image) => (image.uv, Some(ImageKey::of(image))),
                                Paint::Group(group) => {
                                    ([0.0, 0.0, 1.0, 1.0], Some(ImageKey::Group(group.key)))
                                }
                            };
                            let ramp = (q.gradient.as_ref()).map(|g| ramps.place(queue, &g.ramp));
                            Some((GpuQuad::new(q, uv, ramp), image))
                        })
                        .collect()
                })
                .collect()
        };
        let resets = (self.atlas.resets, self.ramps.resets);
        let gpu = once(&mut self.atlas, &mut self.ramps);
        if (self.atlas.resets, self.ramps.resets) == resets {
            return (gpu, false);
        }
        (once(&mut self.atlas, &mut self.ramps), true)
    }

    /// Bind the images the GUI draws that are on the GPU now (`cache`),
    /// and upload the world GUI's canvases `camera` sees this frame in a
    /// `size` window, the untoned ones as screen quads.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &GpuCache<ImageHash, Image>,
        lights: &GpuLights,
        camera: &Camera,
        size: (u32, u32),
    ) -> WorldRuns {
        let overlay = self.world.overlay(camera, size);
        self.overlay.set(device, queue, &overlay);
        self.images.bind(device, &self.overlay.runs, cache);
        self.images.bind(device, &self.screen.runs, cache);
        for (target, _, _) in self.groups.drawn() {
            self.images.bind(device, &target.runs, cache);
        }
        let images = (&self.images, cache);
        self.world
            .upload(device, queue, images, lights, camera, size.1)
    }

    /// Make the world GUI's pipelines again for the parts' new image
    /// layout.
    pub(crate) fn rebuild_world(&mut self, device: &wgpu::Device, layouts: BindLayouts<'_>) {
        self.world.rebuild(device, layouts);
    }

    /// Lay some of the world GUI's canvases in the world.
    pub(crate) fn record_world(&self, pass: &mut wgpu::RenderPass<'_>, runs: &[WorldDraw]) {
        self.world.record(pass, runs);
    }

    /// Draw the groups into their textures, inner ones first, before
    /// anything that shows them.
    pub(crate) fn record_groups(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder) {
        for (target, view, size) in self.groups.drawn() {
            self.draw(queue, encoder, target, view, size, true);
        }
    }

    /// Draw the GUI layer into `target`, if there's any GUI: the untoned
    /// world canvases, then the screen's. Whether there was.
    pub(crate) fn record(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
    ) -> bool {
        let under = self.draw(queue, encoder, &self.overlay, target, size, true);
        let over = self.draw(queue, encoder, &self.screen, target, size, !under);
        under || over
    }

    /// Draw `quads` into `view`, cleared first when `clear`. Whether there
    /// were any.
    fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        quads: &Target,
        view: &wgpu::TextureView,
        (width, height): (u32, u32),
        clear: bool,
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
                    load: match clear {
                        true => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        false => wgpu::LoadOp::Load,
                    },
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
