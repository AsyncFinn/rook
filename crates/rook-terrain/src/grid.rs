//! Terrain's voxels: 4-stud cubes in chunks of 32 on each side, as
//! `SmoothGrid` stores them.

pub mod cells;
mod coarse;
pub(crate) mod decode;
pub mod edit;

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
}

#[derive(Debug, Clone)]
enum Chunk {
    /// Every voxel the same (most chunks of a large map are all air or
    /// all one material).
    Uniform(Voxel),
    /// By `index`.
    Dense(Box<[Voxel]>),
}

/// Where voxel `v` (inside its chunk) is in a dense chunk: Y, then Z, then
/// X, the order `SmoothGrid` stores them in.
fn index(v: IVec3) -> usize {
    (v.y * CHUNK_VOXELS * CHUNK_VOXELS + v.z * CHUNK_VOXELS + v.x) as usize
}

/// The box of a chunk's voxels (from `base`) for which `differs(index)`.
fn differing(base: IVec3, differs: impl Fn(usize) -> bool) -> Option<Bounds> {
    let mut found: Option<Bounds> = None;
    for y in 0..CHUNK_VOXELS {
        for z in 0..CHUNK_VOXELS {
            for x in 0..CHUNK_VOXELS {
                let v = IVec3::new(x, y, z);
                if differs(index(v)) {
                    let v = base + v;
                    found = Some(match found {
                        None => Bounds { min: v, max: v },
                        Some(b) => Bounds {
                            min: b.min.min(v),
                            max: b.max.max(v),
                        },
                    });
                }
            }
        }
    }
    found
}

/// Every voxel of a Terrain. Voxels outside its chunks are air.
#[derive(Debug, Clone, Default)]
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
            *entry = Chunk::Dense(vec![fill; CHUNK_LEN].into_boxed_slice());
        }
        if let Chunk::Dense(voxels) = entry {
            voxels[index(at)] = voxel;
        }
    }

    /// Add a chunk from its voxels in `SmoothGrid` order.
    pub(crate) fn insert_chunk(&mut self, chunk: IVec3, voxels: Vec<Voxel>) {
        debug_assert_eq!(voxels.len(), CHUNK_LEN);
        let first = voxels[0];
        let stored = if voxels.iter().all(|v| *v == first) {
            Chunk::Uniform(first)
        } else {
            Chunk::Dense(voxels.into_boxed_slice())
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

    /// Whether chunk `chunk` is all one voxel, and which.
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

    /// How many voxels of each material the grid holds (Air excluded).
    pub fn counts(&self) -> [u64; 23] {
        let mut counts = [0u64; 23];
        for chunk in self.chunks.values() {
            match chunk {
                Chunk::Uniform(v) => counts[v.material as usize] += CHUNK_LEN as u64,
                Chunk::Dense(voxels) => {
                    for v in voxels.iter() {
                        counts[v.material as usize] += 1;
                    }
                }
            }
        }
        counts[TerrainMaterial::Air as usize] = 0;
        counts
    }

    /// Where the two grids' voxels differ: for each chunk that does, the
    /// box of its voxels that differ. Unordered.
    pub fn changes(&self, other: &Grid) -> Vec<Bounds> {
        let air = Chunk::Uniform(Voxel::AIR);
        let mut out = Vec::new();
        let mut diff = |at: IVec3, a: &Chunk, b: &Chunk| {
            let base = at * CHUNK_VOXELS;
            let whole = Bounds {
                min: base,
                max: base + IVec3::splat(CHUNK_VOXELS - 1),
            };
            let changed = match (a, b) {
                (Chunk::Uniform(a), Chunk::Uniform(b)) => (a != b).then_some(whole),
                (Chunk::Uniform(u), Chunk::Dense(d)) | (Chunk::Dense(d), Chunk::Uniform(u)) => {
                    differing(base, |i| d[i] != *u)
                }
                (Chunk::Dense(a), Chunk::Dense(b)) => {
                    if a == b {
                        None
                    } else {
                        differing(base, |i| a[i] != b[i])
                    }
                }
            };
            out.extend(changed);
        };
        for (at, a) in &self.chunks {
            diff(*at, a, other.chunks.get(at).unwrap_or(&air));
        }
        for (at, b) in &other.chunks {
            if !self.chunks.contains_key(at) {
                diff(*at, &air, b);
            }
        }
        out
    }

    /// Whether every voxel is air.
    pub fn is_empty(&self) -> bool {
        self.counts().iter().all(|&n| n == 0)
    }
}
