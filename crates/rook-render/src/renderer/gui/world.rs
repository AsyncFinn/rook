//! World GUI on the GPU: each SurfaceGui's and BillboardGui's quads drawn
//! straight into the world on its canvas's plane, among the transparent
//! parts by distance (those nearer than every one after the particles),
//! depth tested unless it's AlwaysOnTop, so edges and text stay sharp
//! however near they're seen (docs/behavior/gui/world.md). Untoned
//! canvases (the names over heads) are drawn with the screen's GUI
//! instead, moved to where they are on the screen each frame.
//!
//! Every canvas's quads share one buffer, each beside its canvas's id,
//! and stay there until that canvas changes. Each frame only where each
//! canvas is goes up, into a storage array the quads read by id, so the
//! canvases drawn in a row with one image are one indirect multi-draw, as
//! the parts' runs are: hundreds of canvases cut in among the transparent
//! parts, and a draw with its own buffer and frame offset each cost more
//! to encode than to draw.

mod canvases;
mod frames;
mod recording;

use glam::Vec2;
use rustc_hash::FxHashMap;
use std::ops::Range;
use std::sync::Arc;

use rook_types::{Viewer, WorldCanvas, WorldPlace};
use wesl_core::include_wesl;

use super::super::*;
use super::images::{GuiImages, ImageKey, Run, image_entries, runs};
use super::{GpuQuad, Held, Packed};
use crate::gpu::pool::RangeAlloc;
use crate::renderer::draws::runs::multi_draw;
use crate::renderer::effects::particles::blended_pipeline;
use crate::renderer::pipelines::BindLayouts;
use frames::extent;
use recording::pipelines;

/// How far a SurfaceGui stands off its face, so the two don't fight.
const LIFT: f32 = 0.01;

/// The shared quad buffer's first room, in quads.
const FIRST_QUADS: u32 = 1 << 12;
/// The bytes of a quad, and of the canvas id beside it.
const QUAD: u64 = std::mem::size_of::<GpuQuad>() as u64;
const ID: u64 = 4;

/// Where a canvas is this frame, as the shader reads it: its top left
/// corner and the world step per canvas pixel right and down,
/// then LightInfluence, Brightness and whether it's seen only from in
/// front.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct CanvasFrame {
    origin: [f32; 4],
    right: [f32; 4],
    down: [f32; 4],
    look: [f32; 4],
}

/// One draw of [`wgpu::RenderPass::multi_draw_indirect`]'s list.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct DrawArgs {
    vertex_count: u32,
    instance_count: u32,
    first_vertex: u32,
    first_instance: u32,
}

/// A canvas's quads and their runs, and what it's drawn with.
#[derive(Debug)]
struct Canvas {
    /// The canvas as last given, and the groups it holds.
    source: Arc<WorldCanvas>,
    held: Held,
    /// Its quads' place in the shared buffer (empty: none drawn there),
    /// and its id, where its frame is in the frames' array.
    quads: Range<u32>,
    id: u32,
    /// Its runs, counted from its first quad.
    runs: Vec<Run>,
    /// The corners of a box around every quad, in canvas pixels: quads
    /// can reach past the canvas.
    extent: (Vec2, Vec2),
    size: Vec2,
    light_influence: f32,
    brightness: f32,
    on_top: bool,
    max_distance: f32,
    /// An untoned canvas's quads, kept here to draw with the screen's GUI.
    overlay: Option<Packed>,
}

/// One canvas drawn this frame: its runs' draws in the frame's list,
/// whether it's drawn over everything, how far its middle is from the
/// camera, and where its quads are.
#[derive(Debug, Clone)]
pub(crate) struct WorldDraw {
    draws: Range<u32>,
    pub(crate) on_top: bool,
    pub(crate) distance: f32,
    /// The corners of a box in the world around its quads.
    pub(crate) corners: [Vec3; 4],
}

/// The frame's canvases, back to front, those always on top last.
pub(crate) type WorldRuns = Vec<WorldDraw>;

/// One draw this frame: its arguments, the image it draws with and
/// whether it's on top.
type Draw = (DrawArgs, Option<ImageKey>, bool);

/// Every canvas's quads, and beside each the id of the canvas it's on.
#[derive(Debug)]
struct Quads {
    quads: wgpu::Buffer,
    ids: wgpu::Buffer,
    capacity: u32,
    ranges: RangeAlloc,
}

/// The frames' storage array, how many it holds, and its bind groups,
/// one per image runs draw with.
#[derive(Debug)]
struct Frames {
    buffer: wgpu::Buffer,
    room: u32,
    groups: FxHashMap<Option<ImageKey>, wgpu::BindGroup>,
}

#[derive(Debug)]
pub(crate) struct WorldGui {
    tested: wgpu::RenderPipeline,
    on_top: wgpu::RenderPipeline,
    shader: wgpu::ShaderModule,
    /// Group 3: the light group's entries, then the canvases' frames and
    /// what runs draw with. Groups 0 to 2 are the parts', so drawing
    /// canvases among the transparent parts leaves the parts' images
    /// bound; only this group changes.
    group_layout: wgpu::BindGroupLayout,
    /// The glyph atlas, which text quads draw through, and the
    /// gradients' ramps.
    glyphs: wgpu::TextureView,
    ramps: wgpu::TextureView,
    canvases: Vec<Canvas>,
    /// Where each canvas is now (`None`: not drawn, or not placed yet).
    places: Vec<Option<WorldPlace>>,
    /// The canvas each place is for, as last given, so a canvas given
    /// again keeps its place: only new and moved ones are placed.
    placed: Vec<Arc<WorldCanvas>>,
    quads: Option<Quads>,
    /// The canvases' ids, and each id's frame as last laid out.
    ids: RangeAlloc,
    laid: Vec<CanvasFrame>,
    frames: Option<Frames>,
    /// The frame's draws, in the order drawn, and the buffer they're
    /// uploaded to and its room.
    draws: Vec<Draw>,
    indirect: Option<(wgpu::Buffer, usize)>,
    /// Whether the device draws a list of draws in one call.
    multi_draw: bool,
}

impl WorldGui {
    pub(crate) fn new(
        device: &wgpu::Device,
        layouts: BindLayouts<'_>,
        table: bool,
        (glyphs, ramps): (&wgpu::TextureView, &wgpu::TextureView),
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
        let mut entries = GpuLights::layout_entries().to_vec();
        entries.extend([
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<CanvasFrame>() as u64
                    ),
                },
                count: None,
            },
            texture(5),
            image_entries(6)[0],
            image_entries(6)[1],
            texture(8),
        ]);
        let group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook world gui"),
            entries: &entries,
        });
        let source = if table {
            include_wesl!("world_gui_table")
        } else {
            include_wesl!("world_gui")
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook world gui"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let [tested, on_top] = pipelines(device, layouts, &group_layout, &shader);
        Self {
            tested,
            on_top,
            shader,
            group_layout,
            glyphs: glyphs.clone(),
            ramps: ramps.clone(),
            canvases: Vec::new(),
            places: Vec::new(),
            placed: Vec::new(),
            quads: None,
            ids: RangeAlloc::default(),
            laid: Vec::new(),
            frames: None,
            draws: Vec::new(),
            indirect: None,
            multi_draw: multi_draw(device),
        }
    }

    /// Make the pipelines again for the parts' new image layout (the
    /// table grew).
    pub(crate) fn rebuild(&mut self, device: &wgpu::Device, layouts: BindLayouts<'_>) {
        [self.tested, self.on_top] = pipelines(device, layouts, &self.group_layout, &self.shader);
    }
}
