//! What blocks light in the voxels: the scene's opaque parts as boxes,
//! marked into the light volume's voxels so the fill pass can carry the
//! sky's diffuse light (`EnvironmentDiffuseScale`) down around them, keep
//! lights with `Shadows` out from behind them and shadow the sun; and
//! beside them the terrain, how full of it each voxel is (`terrain.rs`)
//! (`shaders/voxels/light_volume.wgsl`; docs/behavior/rendering/materials.md, "The sky's
//! diffuse light"; docs/behavior/lighting/local-lights.md, "Shadows";
//! docs/behavior/lighting/sun.md).

use std::ops::Range;
use std::time::{Duration, Instant};

use glam::{IVec2, IVec3, Mat3};

use super::slack::Slack;
use super::terrain::{Taken, Terrain};
use super::*;

mod columns;
mod grid;
mod marking;
mod shape;
mod spans;
mod updates;

use columns::*;
use grid::*;
pub(super) use shape::Blocker;
use spans::*;
pub(super) use spans::{center, clip, column_of, minus};

/// How long a frame may spend marking the voxels a moving volume came
/// onto; the rest wait for later frames, the volume staying where it was
/// meanwhile (Studio's grid also moves in chunks, keeping the old one for
/// a while: docs/behavior/lighting/local-lights.md).
const BUDGET: Duration = Duration::from_millis(1);
/// How far from the origin a blocker may reach, in studs: voxel indices
/// stay far inside an i32 (and the box isn't NaN).
const REACH: f32 = (1u32 << 30) as f32 * VOXEL;
/// The pieces those voxels are marked in, across x and z (the volume's
/// full height each), in voxels.
const PIECE: i32 = 16;

#[derive(Debug, Default)]
pub(super) struct Blockers {
    /// By index into `Scene::parts`, at the pose the voxels were marked
    /// with, which a part moving within its slack keeps.
    blockers: Vec<Option<Blocker>>,
    /// How far each may move without marking anything differently, worked
    /// out once a part moves (nothing until then).
    slack: Vec<Slack>,
    /// Where those held at the pose they were marked at while moving
    /// within their slack are now, taken when the volume moves.
    held: FxHashMap<usize, Blocker>,
    /// Where they stand.
    columns: Columns,
    /// Each mesh file's box in its own space (center, half size); `None`
    /// for terrain, which is marked from its voxels instead.
    bounds: FxHashMap<MeshHash, Option<(Vec3, Vec3)>>,
    /// The terrain's voxels.
    terrain: Terrain,
    /// A blocker changed since the voxels were marked.
    changed: bool,
    /// The voxels marked again since the last [`Blockers::mark`] that
    /// finished.
    touched: Touched,
    /// The voxels as last marked, kept so parts that change only unmark
    /// their old box and mark their new one.
    grid: Option<Grid>,
    /// Blockers changed since the grid was marked: (before, after).
    moved: Vec<(Option<Blocker>, Option<Blocker>)>,
    /// Parts changed while nothing used the blockers, taken when
    /// something does; `stale` marks those listed.
    stale: Vec<usize>,
    is_stale: Vec<bool>,
}

/// Voxels from the first corner to the second, both included.
pub(super) type Span = (IVec3, IVec3);

/// Which voxels a mark changed.
#[derive(Debug)]
pub(super) enum Touched {
    /// Those in these boxes, in the world's voxels (within the volume).
    Spans(Vec<Span>),
    /// All of the volume's.
    All,
}

impl Default for Touched {
    fn default() -> Self {
        Self::Spans(Vec::new())
    }
}

impl Touched {
    fn add(&mut self, span: Span) {
        if let Self::Spans(spans) = self {
            spans.push(span);
        }
    }
}

/// The marked voxels, as the fill pass reads them.
#[derive(Debug)]
pub(super) struct Marked<'a> {
    /// A bit per voxel: a blocker holds its center.
    pub(super) bits: &'a [u32],
    /// [`Grid`]'s blocked layers by column.
    pub(super) layers: &'a [u64],
    /// A byte per voxel, four to a word: how full of terrain it is.
    pub(super) soils: &'a [u32],
    /// [`Grid`]'s layers holding terrain, by column.
    pub(super) soil_layers: &'a [u64],
    pub(super) touched: Touched,
    dims: IVec3,
}

impl Blockers {}

#[cfg(test)]
mod tests;
