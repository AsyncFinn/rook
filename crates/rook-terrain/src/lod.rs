//! Level of detail: far terrain drawn from coarser voxels, in rings around
//! the camera, as Studio does and as clipmaps do (Losasso and Hoppe,
//! "Geometry clipmaps", 2004). Each level averages the one before's 2×2×2
//! blocks ([`pyramid`]). The rings' sizes and when they move are in
//! `docs/behavior/terrain/detail.md#how-rook-does-it`; every square sits on
//! the next level's chunk grid, so crossing a ring re-meshes only the
//! chunks whose level changed ([`Rings::follow`]).

mod rings;

pub use rings::Rings;

use glam::{IVec2, IVec3, Vec3};

use crate::grid::edit::Bounds;
use crate::mesh::{Detail, Sides, can_have_surface, chunks_to_mesh};
use crate::{CHUNK_VOXELS, Grid};

/// Levels of detail: cells of 4 to 128 studs. Studio's measured rings go
/// to 64 and on; the last level's chunks are 4,096 studs, so a place of
/// any size draws only a few of them.
pub const LEVELS: u8 = 6;

/// The coarsest level, drawn everywhere the others aren't.
const LAST: u8 = LEVELS - 1;

/// How much farther than a chunk the camera goes from level 0's center
/// before its square moves, in chunks.
const SLACK: f32 = 1.0 / 8.0;

/// Level `k`'s square, in its chunks on a side.
const fn width(level: usize) -> i32 {
    if level == 0 { 4 } else { 6 }
}

pub fn chunk_studs(level: u8) -> f32 {
    CHUNK_VOXELS as f32 * Detail::at(level).voxel_studs()
}

/// How close to the camera, across X and Z, a chunk drawn at `level` can
/// be: level 0's square reaches two of its chunks each way from its
/// center, which strays at most a chunk and [`SLACK`] from the camera, and
/// each square after reaches at least one of its chunks past the one
/// inside.
pub fn nearest(level: u8) -> f32 {
    match level {
        0 => 0.0,
        k => chunk_studs(0) * (1.0 - SLACK) + (1..k).map(chunk_studs).sum::<f32>(),
    }
}

/// The voxels of every level, the 4-stud ones first.
pub fn pyramid(grid: Grid) -> Vec<Grid> {
    let mut levels = vec![grid];
    for _ in 1..LEVELS {
        let coarse = levels.last().expect("level 0").coarse();
        levels.push(coarse);
    }
    levels
}

/// Average again the coarse voxels over `changed` (voxels of level 0 that
/// changed in `levels[0]`). Returns, for each level, the voxels of it that
/// changed.
pub fn update_pyramid(levels: &mut [Grid], changed: &[Bounds]) -> Vec<Vec<Bounds>> {
    let mut out = vec![changed.to_vec()];
    for k in 1..levels.len() {
        let coarse: Vec<Bounds> = out[k - 1]
            .iter()
            .map(|b| Bounds {
                min: b.min.div_euclid(IVec3::splat(2)),
                max: b.max.div_euclid(IVec3::splat(2)),
            })
            .collect();
        let (finer, rest) = levels.split_at_mut(k);
        for b in &coarse {
            rest[0].coarsen(&finer[k - 1], *b);
        }
        out.push(coarse);
    }
    out
}

#[cfg(test)]
mod tests;
