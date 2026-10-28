//! A grid at half the resolution: each voxel of it stands for a 2×2×2
//! block of the finer one, as far terrain is meshed
//! (`docs/behavior/terrain/detail.md`). A coarse voxel holds the eight's
//! mean occupancy of whichever of solid or water holds more (water
//! counting what solid voxels hold), solids taking the material with the most occupancy among them, so a coarse
//! surface lies where the fine one does, half a coarse voxel out instead
//! of half a fine one (Studio's coarser rings sit higher too).

use glam::IVec3;

use crate::TerrainMaterial;
use crate::grid::{CHUNK_LEN, CHUNK_VOXELS, Grid, Voxel, index};

/// Fine voxels along each edge of a coarse chunk.
const SPAN: i32 = 2 * CHUNK_VOXELS;

impl Grid {
    /// The coarse chunks holding some of `fine`'s stored chunks, which
    /// [`Grid::coarse_chunk`] makes.
    pub fn coarse_chunks(fine: &Grid) -> Vec<IVec3> {
        let mut out: Vec<IVec3> = fine
            .chunks()
            .map(|c| c.div_euclid(IVec3::splat(2)))
            .collect();
        out.sort_by_key(|c| (c.x, c.y, c.z));
        out.dedup();
        out
    }

    /// Coarse chunk `chunk` of `fine`, from its eight fine chunks: `None`
    /// when it's all air.
    pub fn coarse_chunk(fine: &Grid, chunk: IVec3) -> Option<Vec<Voxel>> {
        let first = chunk * 2;
        let corners = (0..8).map(|i| first + IVec3::new(i & 1, i >> 1 & 1, i >> 2));
        let mut uniform = corners.map(|c| fine.uniform(c));
        let one = uniform.next().flatten();
        if let Some(v) = one
            && uniform.all(|u| u == Some(v))
        {
            // Eight of the same voxel average to it.
            return (v != Voxel::AIR).then(|| vec![v; CHUNK_LEN]);
        }
        let mut block = Vec::new();
        fine.read_block(chunk * SPAN, IVec3::splat(SPAN), &mut block);
        let at = |p: IVec3| block[(p.x + SPAN * (p.y + SPAN * p.z)) as usize];
        let mut out = vec![Voxel::AIR; CHUNK_LEN];
        for z in 0..CHUNK_VOXELS {
            for y in 0..CHUNK_VOXELS {
                for x in 0..CHUNK_VOXELS {
                    let v = IVec3::new(x, y, z);
                    out[index(v)] =
                        average((0..8).map(|i| at(v * 2 + IVec3::new(i & 1, i >> 1 & 1, i >> 2))));
                }
            }
        }
        out.iter().any(|v| *v != Voxel::AIR).then_some(out)
    }

    /// Put a coarse chunk made by [`Grid::coarse_chunk`] in place, or
    /// clear it.
    pub fn put_chunk(&mut self, chunk: IVec3, voxels: Option<Vec<Voxel>>) {
        match voxels {
            Some(v) => self.insert_chunk(chunk, v),
            None => {
                self.chunks.remove(&chunk);
            }
        }
    }

    /// The whole grid at half the resolution.
    pub fn coarse(&self) -> Grid {
        let mut out = Grid::new();
        for c in Grid::coarse_chunks(self) {
            out.put_chunk(c, Grid::coarse_chunk(self, c));
        }
        out
    }
}

/// The coarse voxel of eight fine ones.
fn average(voxels: impl Iterator<Item = Voxel>) -> Voxel {
    let mut solid = [0.0f32; TerrainMaterial::ALL.len()];
    let mut water = 0.0;
    for v in voxels {
        if v.is_solid() {
            solid[v.material as usize] += v.fill();
            water += v.water_fill();
        } else if v.material == TerrainMaterial::Water {
            water += v.fill();
        }
    }
    let solids: f32 = solid.iter().sum();
    if solids == 0.0 && water == 0.0 {
        return Voxel::AIR;
    }
    if solids >= water {
        // The first by index on a tie, as a cell's dominant material.
        let best = (0..solid.len())
            .rev()
            .max_by(|&a, &b| solid[a].total_cmp(&solid[b]))
            .unwrap_or(0);
        return Voxel {
            material: TerrainMaterial::ALL[best],
            occupancy: byte(solids / 8.0),
            water: byte(water / 8.0),
        };
    }
    // Mostly water: solids count as inside for the water's surface.
    Voxel::new(TerrainMaterial::Water, byte((solids + water) / 8.0))
}

/// The occupancy byte nearest `fill` (byte `b` is `(b + 1) / 256` full).
fn byte(fill: f32) -> u8 {
    ((fill * 256.0).round() - 1.0).clamp(0.0, 255.0) as u8
}
