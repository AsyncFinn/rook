//! Level of detail: far terrain drawn from coarser voxels, in rings
//! around the camera, as Studio does (`docs/behavior/terrain/detail.md`)
//! and as clipmaps do (Losasso and Hoppe, "Geometry clipmaps", 2004).
//!
//! Each level averages the one before's 2×2×2 blocks ([`pyramid`]).
//! Level `k` draws a square of columns of its chunks, every chunk in them
//! however high or low, less the square of level `k - 1`; the last level
//! draws everything outside. Level 0's square is 4×4 around the camera and
//! moves only once the camera is more than a chunk (and a little) from its
//! center; each square after is 6×6 and holds the one inside with at least
//! one of its own chunks on every side, so no level is ever skipped (as
//! clipmaps keep a border of each level around the next finer), and moves
//! only when that one would leave it. Every square sits on the next
//! level's chunk grid, so crossing a ring re-meshes only the chunks whose
//! level changed ([`Rings::follow`]).

use glam::{IVec2, IVec3, Vec3};

use crate::grid::edit::Bounds;
use crate::mesh::{Detail, can_have_surface, chunks_to_mesh};
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

/// A chunk's edge in studs at `level`.
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
        let mut chunks: Vec<IVec3> = coarse.iter().flat_map(|b| b.chunks(0)).collect();
        chunks.sort_by_key(|c| (c.x, c.y, c.z));
        chunks.dedup();
        let (finer, rest) = levels.split_at_mut(k);
        for c in chunks {
            rest[0].put_chunk(c, Grid::coarse_chunk(&finer[k - 1], c));
        }
        out.push(coarse);
    }
    out
}

/// Where each level's square is, but the last's: its first chunk across X
/// and Z, in that level's chunks (an even one, a corner of the next
/// level's chunks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rings {
    starts: [IVec2; LAST as usize],
}

impl Rings {
    /// The rings around `camera`.
    pub fn around(camera: Vec3) -> Self {
        let mut rings = Rings {
            starts: [IVec2::ZERO; LAST as usize],
        };
        rings.place(camera, true);
        rings
    }

    /// Move the squares the camera has left. Returns whether any moved.
    pub fn follow(&mut self, camera: Vec3) -> bool {
        let old = *self;
        self.place(camera, false);
        *self != old
    }

    fn place(&mut self, camera: Vec3, fresh: bool) {
        // Level 0 follows the camera; each after holds the one inside.
        let at = glam::Vec2::new(camera.x, camera.z) / chunk_studs(0);
        let half = (width(0) / 2) as f32;
        let center = self.starts[0].as_vec2() + half;
        if fresh || (at - center).abs().max_element() > 1.0 + SLACK {
            self.starts[0] = even((at - half) / 2.0);
        }
        for k in 1..LAST as usize {
            let at = glam::Vec2::new(camera.x, camera.z) / chunk_studs(k as u8);
            let inner = self.starts[k - 1] / 2;
            let reach = width(k - 1) / 2;
            // Starts that leave a chunk of this level each side of the one
            // inside: from `inner + reach + 1 - width` to `inner - 1`.
            let lo = inner + reach + 1 - width(k);
            let hi = inner - 1;
            let fits = |s: IVec2| s.cmpge(lo).all() && s.cmple(hi).all();
            if fresh || !fits(self.starts[k]) {
                let want = even((at - (width(k) / 2) as f32) / 2.0);
                let lo = IVec2::new(round_up_even(lo.x), round_up_even(lo.y));
                let hi = IVec2::new(round_down_even(hi.x), round_down_even(hi.y));
                self.starts[k] = want.clamp(lo, hi);
            }
        }
    }

    /// Whether chunk `chunk` of `level` is drawn at that level.
    pub fn draws(&self, level: u8, chunk: IVec3) -> bool {
        let column = IVec2::new(chunk.x, chunk.z);
        let within = |lo: IVec2, hi: IVec2| column.cmpge(lo).all() && column.cmplt(hi).all();
        let k = usize::from(level);
        let outer = level == LAST || within(self.starts[k], self.starts[k] + width(k));
        let inner = level > 0 && {
            let lo = self.starts[k - 1] / 2;
            within(lo, lo + width(k - 1) / 2)
        };
        outer && !inner
    }

    /// The chunks of `level` drawn at it that can have a surface, `grid`
    /// holding that level's voxels and `rows` the lowest and highest
    /// level-0 chunk rows that can, in chunk order.
    pub fn chunks(&self, level: u8, grid: &Grid, rows: (i32, i32)) -> Vec<IVec3> {
        if level == LAST {
            return chunks_to_mesh(grid)
                .into_iter()
                .filter(|c| self.draws(level, *c))
                .collect();
        }
        let first = self.starts[usize::from(level)];
        let side = width(usize::from(level));
        let step = 1 << level;
        let (lo, hi) = (rows.0.div_euclid(step) - 1, rows.1.div_euclid(step));
        let mut out = Vec::new();
        for x in 0..side {
            for y in lo..=hi {
                for z in 0..side {
                    let c = IVec3::new(first.x + x, y, first.y + z);
                    if self.draws(level, c) && can_have_surface(grid, c) {
                        out.push(c);
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;

/// The even point nearest `v` (in chunks), times 2: a corner of the next
/// level's chunks.
fn even(v: glam::Vec2) -> IVec2 {
    v.round().as_ivec2() * 2
}

fn round_up_even(v: i32) -> i32 {
    v + v.rem_euclid(2)
}

fn round_down_even(v: i32) -> i32 {
    v - v.rem_euclid(2)
}
