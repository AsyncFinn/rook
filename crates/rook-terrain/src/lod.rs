//! Level of detail: far terrain drawn from coarser voxels, in rings
//! around the camera, as Studio does (`docs/behavior/terrain/detail.md`)
//! and as clipmaps do (Losasso and Hoppe, "Geometry clipmaps", 2004).
//!
//! Level 0 is the 4-stud voxels; each level after averages the one
//! before's 2×2×2 blocks ([`pyramid`]), so its voxels are twice the size
//! and so are its chunks of 32. Level `k` draws a box of 4×4×4 of its
//! chunks around the camera (half-width 256 studs at level 0, doubling)
//! less the box of level `k - 1` inside it; the last level draws all its
//! chunks outside the box before it, however far. Each box sits on the
//! grid of the next level's chunks, so the hole it leaves in that level
//! is whole chunks, and only moves when the camera has gone more than
//! half that chunk (and a little, so it doesn't flicker back and forth)
//! from its center: crossing a ring re-meshes only the chunks whose level
//! changed ([`Rings::follow`]).

use glam::{IVec3, Vec3};

use crate::grid::edit::Bounds;
use crate::mesh::{Detail, can_have_surface, chunks_to_mesh};
use crate::{CHUNK_VOXELS, Grid};

/// Levels of detail: cells of 4 to 128 studs. Studio's measured rings go
/// to 64 and on; the last level's chunks are 4,096 studs, so a place of
/// any size draws only a few of them.
pub const LEVELS: u8 = 6;

/// The coarsest level, drawn everywhere the others aren't.
const LAST: u8 = LEVELS - 1;

/// How much farther than half a chunk the camera goes from a box's center
/// before the box moves, in chunks.
const SLACK: f32 = 1.0 / 16.0;

/// A chunk's edge in studs at `level`.
pub fn chunk_studs(level: u8) -> f32 {
    CHUNK_VOXELS as f32 * Detail::at(level).voxel_studs()
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

/// Where each level's box is: for levels 0 to the one before the last,
/// its center, a corner of the next level's chunks (in those chunks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rings {
    centers: [IVec3; LAST as usize],
}

impl Rings {
    /// The rings around `camera`.
    pub fn around(camera: Vec3) -> Self {
        let mut rings = Rings {
            centers: [IVec3::ZERO; LAST as usize],
        };
        rings.place(camera, true);
        rings
    }

    /// Move the boxes the camera has left. Returns whether any moved.
    pub fn follow(&mut self, camera: Vec3) -> bool {
        let old = *self;
        self.place(camera, false);
        *self != old
    }

    fn place(&mut self, camera: Vec3, fresh: bool) {
        for k in (0..LAST as usize).rev() {
            let at = camera / chunk_studs(k as u8 + 1);
            let center = &mut self.centers[k];
            if fresh || (at - center.as_vec3()).abs().max_element() > 0.5 + SLACK {
                *center = at.round().as_ivec3();
            }
            // Inside the next box, so the levels nest.
            if let Some(outer) = self.centers.get(k + 1).map(|c| *c * 2) {
                self.centers[k] = self.centers[k].clamp(outer - 1, outer + 1);
            }
        }
    }

    /// Whether chunk `chunk` of `level` is drawn at that level.
    pub fn draws(&self, level: u8, chunk: IVec3) -> bool {
        let within = |lo: IVec3, hi: IVec3| chunk.cmpge(lo).all() && chunk.cmplt(hi).all();
        let k = usize::from(level);
        let outer = level == LAST || within(self.centers[k] * 2 - 2, self.centers[k] * 2 + 2);
        let inner = level > 0 && within(self.centers[k - 1] - 1, self.centers[k - 1] + 1);
        outer && !inner
    }

    /// The chunks of `level` drawn at it that can have a surface, `grid`
    /// holding that level's voxels, in chunk order.
    pub fn chunks(&self, level: u8, grid: &Grid) -> Vec<IVec3> {
        if level == LAST {
            return chunks_to_mesh(grid)
                .into_iter()
                .filter(|c| self.draws(level, *c))
                .collect();
        }
        let first = self.centers[usize::from(level)] * 2 - 2;
        let mut out = Vec::new();
        for x in 0..4 {
            for y in 0..4 {
                for z in 0..4 {
                    let c = first + IVec3::new(x, y, z);
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
