mod assets;
mod draws;
mod effects;
mod gui;
mod images;
mod inputs;
mod passes;
mod pipelines;
mod readback;
mod setup;
mod stats;
mod timing;

use std::sync::Arc;

use super::*;
use crate::lighting::local::GpuLights;
use crate::lighting::sky::SkyCube;
use draws::placement::Placement;
use effects::particles::GpuParticles;
use effects::ribbons::GpuRibbons;
use gui::GuiPass;
use gui::world::WorldRuns;
pub(crate) use images::ImageBinding;
use images::{Defaults, image_layout, image_set};
pub(crate) use images::{ImageSet, lone_image};
use passes::highlights::HighlightParts;
use pipelines::{BindLayouts, Pipelines};
use timing::GpuTimer;

use draws::worlds::World;
use passes::tonemap::{FRAME_FORMAT, Tonemap};
use passes::transparent::TransparentParts;
use passes::viewports::Viewports;
pub(crate) use setup::adapter;

/// Water's phase wraps here: a whole number of its 25 normal frames'
/// cycles (20 units each) and of both waves' periods (40 and 25,
/// `shaders/terrain.wgsl`), so nothing jumps.
const WATER_PHASE_PERIOD: f64 = 200.0;

/// GPU objects shared by windowed and offscreen rendering.
#[derive(Debug)]
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// The GPU, its backend and driver, for logs and crash reports.
    gpu: String,
    color_format: wgpu::TextureFormat,
    /// See [`UploadContext::view_formats`].
    view_formats: bool,
    /// See [`UploadContext::block_compression`].
    pub(crate) block_compression: bool,
    pipelines: Pipelines,
    globals: wgpu::Buffer,
    globals_layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    /// The shadow map, drawn by the shadow pass and sampled by the main
    /// pass through `main_bind_group`, with the surface patterns, water's
    /// normal frames, the sky's reflection cube and the BRDF table.
    shadow_view: wgpu::TextureView,
    surfaces_view: wgpu::TextureView,
    water_view: wgpu::TextureView,
    sky_cube: SkyCube,
    brdf_view: wgpu::TextureView,
    shadow_sampler: wgpu::Sampler,
    main_layout: wgpu::BindGroupLayout,
    main_bind_group: wgpu::BindGroup,
    image_layout: wgpu::BindGroupLayout,
    image_sampler: wgpu::Sampler,
    /// What stands in for images a draw doesn't have.
    defaults: Defaults,
    /// The image table, or each image set's bind group (group 2).
    pub(crate) image_binding: ImageBinding,
    /// The built-in shapes and each mesh file a part has drawn, by shape,
    /// each a place in `pool`.
    meshes: GpuCache<DrawShape, Mesh>,
    pool: MeshPool,
    /// Whether each pipeline's draws go in multi-draws from `indirect`
    /// (the adapter takes a first instance in indirect draws), or one by
    /// one.
    pub(crate) multi_draw: bool,
    /// The frame's draws' arguments, and how many it has room for.
    indirect: Option<(wgpu::Buffer, usize)>,
    /// Where world canvases cut in among the transparent parts.
    cuts: passes::cuts::Cuts,
    /// Each image something has drawn, by content hash.
    images: GpuCache<ImageHash, Image>,
    /// Workspace's world.
    world: World,
    /// Each ViewportFrame's world, and its picture.
    viewports: Viewports,
    /// Uploads the instances that changed to their slots.
    scatter: crate::batch::scatter::Scatter,
    /// Which world each entry of the scene is drawn in, and how, by its
    /// index in the scene.
    placements: Vec<(Space, Placement)>,
    /// Which level of detail each entry's mesh draws at.
    detail: draws::detail::Detail,
    /// Entries waiting for an upload, placed once it's done.
    waiting: draws::waiting::Waiting,
    /// The parts Highlights cover, drawn into their mask.
    highlights: HighlightParts,
    lighting: Lighting,
    /// The local lights and their spatial hash (group 3).
    pub(crate) lights: GpuLights,
    /// The emitters' particles.
    particles: GpuParticles,
    /// Beams and Trails.
    ribbons: GpuRibbons,
    terrain: TerrainLook,
    /// Seconds on the clock grass sways by ([`Renderer::set_time`]).
    time: f32,
    /// How far water has moved: `WaterWaveSpeed` × seconds, summed as the
    /// speed changes (observed: Studio's water stops where it is at speed
    /// 0 and goes on from there), wrapped at [`WATER_PHASE_PERIOD`].
    pub(crate) water_phase: f64,
    depth: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
    /// The frame the main pass draws into, and the pass taking it to the
    /// target.
    tonemap: Tonemap,
    gui: GuiPass,
    /// The last frame's draw calls.
    draw_counts: DrawCounts,
    /// The GPU's pass times, once asked for ([`Renderer::time_gpu`]).
    timer: Option<GpuTimer>,
    /// How long the last frame took the CPU to record and submit, file
    /// uploads included.
    record_time: std::time::Duration,
    stage_times: StageTimes,
}

/// Where the CPU's time recording the last frame went.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageTimes {
    /// Mesh files and images going up, and entries waiting on them placed.
    pub uploads: std::time::Duration,
    /// Shadow view, image table, globals.
    pub setup: std::time::Duration,
    /// Changed instances uploaded, transparent parts sorted.
    pub instances: std::time::Duration,
    /// Beams, Trails and particles built and uploaded.
    pub effects: std::time::Duration,
    /// Culling and the draws' arguments.
    pub draws: std::time::Duration,
    /// The light voxels filled, when the lights or the parts blocking them
    /// changed or the view moved on (the frame's encoder made first).
    pub lights: std::time::Duration,
    /// Recording the passes and submitting them.
    pub passes: std::time::Duration,
}

/// What a frame drew, in the shadow pass and the main pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DrawCounts {
    pub shadow: PassCounts,
    pub main: PassCounts,
}

/// What a pass drew: the draw calls recorded (a multi-draw is one), the
/// instanced draws in them, and their triangles (free instance slots
/// included).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PassCounts {
    pub calls: u32,
    pub draws: u32,
    pub triangles: u64,
}

impl Renderer {}
