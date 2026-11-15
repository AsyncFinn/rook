//! Terrain's voxels: 4-stud cubes in chunks of 32 on each side, as
//! `SmoothGrid` stores them.

pub mod cells;
mod coarse;
pub(crate) mod decode;
pub mod edit;
mod regions;

use std::sync::Arc;

use glam::IVec3;
use rustc_hash::FxHashMap;

use crate::TerrainMaterial;
use crate::grid::edit::Bounds;

/// A voxel's edge, in studs. Voxel `v` covers `4v` to `4v + 4`.
pub const VOXEL_STUDS: f32 = 4.0;

/// Voxels along each edge of a chunk.
pub const CHUNK_VOXELS: i32 = 32;

const CHUNK_LEN: usize = (CHUNK_VOXELS * CHUNK_VOXELS * CHUNK_VOXELS) as usize;

/// One voxel. Occupancies are bytes as `SmoothGrid` stores them: byte `b`
/// is `(b + 1) / 256` full, so 255 is full (Confirmed: a place built with
/// known bytes reads back in Studio's `ReadVoxels` as exactly that).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Voxel {
    pub material: TerrainMaterial,
    /// How full of its material it is (of water, for a Water voxel).
    pub occupancy: u8,
    /// How full of water the rest of a solid voxel is (shorelines).
    pub water: u8,
}

impl Voxel {
    pub const AIR: Voxel = Voxel {
        material: TerrainMaterial::Air,
        occupancy: 0,
        water: 0,
    };

    /// A voxel of `material`; occupancy byte 0 is still 1/256 full.
    pub fn new(material: TerrainMaterial, occupancy: u8) -> Self {
        if material == TerrainMaterial::Air {
            return Self::AIR;
        }
        Self {
            material,
            occupancy,
            water: 0,
        }
    }

    /// Made of a solid material: not Air or Water.
    pub fn is_solid(self) -> bool {
        !matches!(self.material, TerrainMaterial::Air | TerrainMaterial::Water)
    }

    /// Its occupancy from 0 to 1, as `ReadVoxels` gives it.
    pub fn fill(self) -> f32 {
        (f32::from(self.occupancy) + 1.0) / 256.0
    }

    /// Holding water: a Water voxel, or a solid one with water in it.
    pub fn is_wet(self) -> bool {
        self.material == TerrainMaterial::Water || self.is_solid() && self.water > 0
    }

    /// How full of water a solid voxel is, from 0 to 1, read as a Water
    /// voxel's occupancy byte is (0 is dry).
    pub fn water_fill(self) -> f32 {
        if self.water == 0 {
            return 0.0;
        }
        (f32::from(self.water) + 1.0) / 256.0
    }

    /// How far water's surface reaches out of this voxel, from 0 to 1 as
    /// an occupancy: a Water voxel's occupancy, the water a solid holds, a
    /// whole voxel for a dry solid, none for air. Solids count as inside
    /// water's surface, a dry one wholly (observed: at shores where the
    /// partly filled solids hold no water, Studio's water stays flat at
    /// its level up to where the land rises through it, and a dry Rock
    /// voxel in a slab's top leaves no dip), so water meets land where
    /// the two surfaces cross (`docs/behavior/terrain/water.md#shorelines`).
    pub fn water_surface_fill(self) -> f32 {
        match self.material {
            TerrainMaterial::Air => 0.0,
            TerrainMaterial::Water => self.fill(),
            _ if self.water > 0 => self.water_fill(),
            _ => 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Chunk {
    /// Every voxel the same (most chunks of a large map are all air or
    /// all one material).
    Uniform(Voxel),
    /// By `index`, shared by the grids copied from this one until one of
    /// them writes to it (copy on write): the DataModel's voxels, physics'
    /// and the renderer's are one copy, and an edit copies only the chunks
    /// it writes.
    Dense(Arc<[Voxel]>),
}

/// Where voxel `v` (inside its chunk) is in a dense chunk: Y, then Z, then
/// X, the order `SmoothGrid` stores them in.
fn index(v: IVec3) -> usize {
    (v.y * CHUNK_VOXELS * CHUNK_VOXELS + v.z * CHUNK_VOXELS + v.x) as usize
}

/// Every voxel of a Terrain. Voxels outside its chunks are air. Cloning
/// shares the chunks' voxels, at the cost of the chunk count.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Grid {
    chunks: FxHashMap<IVec3, Chunk>,
}

/// The chunk holding voxel `v`, and where `v` is inside it.
pub fn chunk_of(v: IVec3) -> (IVec3, IVec3) {
    (
        v.div_euclid(IVec3::splat(CHUNK_VOXELS)),
        v.rem_euclid(IVec3::splat(CHUNK_VOXELS)),
    )
}

impl Grid {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn voxel(&self, v: IVec3) -> Voxel {
        let (chunk, at) = chunk_of(v);
        match self.chunks.get(&chunk) {
            None => Voxel::AIR,
            Some(Chunk::Uniform(voxel)) => *voxel,
            Some(Chunk::Dense(voxels)) => voxels[index(at)],
        }
    }

    pub fn set(&mut self, v: IVec3, voxel: Voxel) {
        let (chunk, at) = chunk_of(v);
        if voxel == Voxel::AIR && !self.chunks.contains_key(&chunk) {
            return;
        }
        let entry = self
            .chunks
            .entry(chunk)
            .or_insert(Chunk::Uniform(Voxel::AIR));
        if let Chunk::Uniform(fill) = *entry {
            if fill == voxel {
                return;
            }
            *entry = Chunk::Dense(vec![fill; CHUNK_LEN].into());
        }
        if let Chunk::Dense(voxels) = entry {
            Arc::make_mut(voxels)[index(at)] = voxel;
        }
    }

    /// Add a chunk from its voxels in `SmoothGrid` order.
    pub(crate) fn insert_chunk(&mut self, chunk: IVec3, voxels: Vec<Voxel>) {
        debug_assert_eq!(voxels.len(), CHUNK_LEN);
        let first = voxels[0];
        let stored = if voxels.iter().all(|v| *v == first) {
            Chunk::Uniform(first)
        } else {
            Chunk::Dense(voxels.into())
        };
        self.chunks.insert(chunk, stored);
    }

    /// The chunks the grid stores (others are all air), unordered.
    pub fn chunks(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.chunks.keys().copied()
    }

    /// A stored chunk's voxels in `SmoothGrid` order (Y slowest, X
    /// fastest), in chunk order.
    pub(crate) fn stored(&self) -> Vec<(IVec3, std::borrow::Cow<'_, [Voxel]>)> {
        let mut out: Vec<_> = self
            .chunks
            .iter()
            .map(|(at, chunk)| {
                let voxels = match chunk {
                    Chunk::Uniform(v) => std::borrow::Cow::Owned(vec![*v; CHUNK_LEN]),
                    Chunk::Dense(voxels) => std::borrow::Cow::Borrowed(&voxels[..]),
                };
                (*at, voxels)
            })
            .collect();
        out.sort_by_key(|(c, _)| (c.x, c.y, c.z));
        out
    }

    pub fn uniform(&self, chunk: IVec3) -> Option<Voxel> {
        match self.chunks.get(&chunk) {
            None => Some(Voxel::AIR),
            Some(Chunk::Uniform(voxel)) => Some(*voxel),
            Some(Chunk::Dense(_)) => None,
        }
    }

    /// Copy the voxels from `min` (inclusive) to `min + size` into `out`,
    /// X fastest, then Y, then Z, a chunk at a time.
    pub fn read_block(&self, min: IVec3, size: IVec3, out: &mut Vec<Voxel>) {
        out.clear();
        out.resize((size.x * size.y * size.z) as usize, Voxel::AIR);
        let max = min + size - IVec3::ONE;
        let (first, _) = chunk_of(min);
        let (last, _) = chunk_of(max);
        for cz in first.z..=last.z {
            for cy in first.y..=last.y {
                for cx in first.x..=last.x {
                    let chunk = IVec3::new(cx, cy, cz);
                    let base = chunk * CHUNK_VOXELS;
                    let lo = min.max(base);
                    let hi = max.min(base + IVec3::splat(CHUNK_VOXELS - 1));
                    let stored = self.chunks.get(&chunk);
                    for z in lo.z..=hi.z {
                        for y in lo.y..=hi.y {
                            let row = ((z - min.z) * size.y + (y - min.y)) * size.x - min.x;
                            for x in lo.x..=hi.x {
                                let voxel = match stored {
                                    None => continue,
                                    Some(Chunk::Uniform(v)) => *v,
                                    Some(Chunk::Dense(voxels)) => {
                                        voxels[index(IVec3::new(x, y, z) - base)]
                                    }
                                };
                                out[(row + x) as usize] = voxel;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.counts().iter().all(|&n| n == 0)
    }
}
