//! Local lights on the GPU, drawn as Studio's voxel lighting draws them:
//! every light in one buffer, a spatial hash saying which lights reach each
//! place (reaches padded by [`SLACK`], so lights that move a little don't
//! rebuild it), and a volume of 4-stud voxels ahead of the camera that a
//! compute pass fills (`shaders/voxels/light_volume.wgsl`) and surfaces sample
//! (`shaders/lighting.wgsl`). The voxels are filled again only where a
//! light or a part changed (`tiles.rs`), and, when the camera leaves the
//! volume's middle and it follows, only what it came onto and what that
//! reaches: they are kept anchored to the world, wrapping around as
//! Studio's light map does. Parts marked into them (`blockers.rs`) carry the
//! sky's light down, shadow lights and, without
//! `PrioritizeLightingQuality`, shadow the sun through cones that soften
//! with distance (`cover.rs`); terrain marked into them (`terrain.rs`) dims
//! the sky's light and the sun. What Studio does is in
//! `docs/behavior/lighting/local-lights.md` and `docs/behavior/lighting/sun.md`.

mod blockers;
mod changes;
mod cover;
mod fill;
mod hash;
mod resources;
mod slack;
mod terrain;
mod tiles;

use std::ops::Range;

use wesl_core::include_wesl;

use super::*;
use blockers::{Blockers, Span, Touched};
use changes::*;
use cover::{CONE_START, CONE_WIDENS, Changed, Cover, LEVELS, cone_radius, footprint_reach};
use fill::*;
use glam::{IVec2, IVec3, Vec2};
use hash::{GpuLight, LightHash};
use resources::*;
use tiles::{ACROSS, TILE, Tiles, WORDS};

/// How far a light may move from where the hash placed it before the hash
/// is built again, in studs.
pub(super) const SLACK: f32 = 8.0;
/// Studio's light voxels, in studs (centered at 4k + 2).
const VOXEL: f32 = 4.0;
/// The voxel volume: voxels along each axis (1024 x 256 x 1024 studs, as
/// Studio's grid: docs/behavior/lighting/local-lights.md). Light stops at its faces.
const VOLUME: [u32; 3] = [256, 64, 256];
/// How far ahead of the camera's Focus the volume's center sits, along the
/// view direction with its height dropped (Studio's grid reaches about 760
/// studs ahead of its edit camera, whose Focus is 20 ahead, and 260 behind;
/// looking straight down, it's centered).
const AHEAD: f32 = 236.0;
/// How far the volume's center may drift from where it should be before
/// the volume follows, in studs. It moves by whole tiles (`snapped`), as
/// Studio's grid moves in chunks.
const RECENTER: f32 = 64.0;
/// How far the sun may turn (the cosine of the angle) before the voxels
/// take its light again; a script turning `ClockTime` would otherwise
/// fill them every frame.
const SUN_TURN: f32 = 0.99995;

/// The lights' buffers and hash, the voxel volume and the pass that fills
/// it, and the main pass's bind group (group 3).
#[derive(Debug)]
pub(crate) struct GpuLights {
    pub(crate) layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    /// No volume at all: no local lights, sky or sun voxels, for worlds
    /// none of Workspace's lights reach (ViewportFrames').
    pub(crate) unlit: wgpu::BindGroup,
    lights: Vec<GpuLight>,
    /// How many of `lights` have `Shadows`.
    shadowed: usize,
    /// Where each light was when the hash was built.
    hashed_at: Vec<Vec3>,
    hash: LightHash,
    light_buffer: wgpu::Buffer,
    bucket_buffer: wgpu::Buffer,
    entry_buffer: wgpu::Buffer,
    params: wgpu::Buffer,
    volume: wgpu::Buffer,
    voxels: wgpu::TextureView,
    /// How much of the sun reaches each voxel (red), past the parts.
    sun_voxels: wgpu::TextureView,
    sampler: wgpu::Sampler,
    fill: wgpu::ComputePipeline,
    fill_layout: wgpu::BindGroupLayout,
    fill_group: wgpu::BindGroup,
    sky_buffers: SkyBuffers,
    /// How much of the sun each voxel stops, which the sun's cones read.
    cover: Cover,
    blockers: Blockers,
    /// The center of voxel (0, 0, 0), once filled.
    origin: Option<Vec3>,
    /// What changed since the voxels were filled, besides the blockers.
    dirty: Dirty,
    /// The voxels hold the sky's light.
    sky: bool,
    /// Some light has `Shadows`.
    shadows: bool,
    /// The sun (towards it) the voxels were filled with, if they hold its
    /// light.
    sun: Option<Vec3>,
    /// The blockers are marked for the volume where it is now.
    marked: bool,
    /// The tiles the last fill filled, over every layer.
    filled: u32,
}

impl GpuLights {}
