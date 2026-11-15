//! Whole regions of the grid: material counts, chunks and regions copied
//! between grids, and the bounds where two grids differ.

use super::*;

impl Grid {
    /// How many voxels of each material the grid holds (Air excluded).
    pub fn counts(&self) -> [u64; 23] {
        let mut counts = [0u64; 23];
        for chunk in self.chunks.values() {
            add_counts(chunk, &mut counts);
        }
        counts[TerrainMaterial::Air as usize] = 0;
        counts
    }

    /// How many voxels of each material chunk `c` holds (Air excluded):
    /// what a change to it adds to or takes from [`Grid::counts`].
    pub fn chunk_counts(&self, c: IVec3) -> [u64; 23] {
        let mut counts = [0u64; 23];
        if let Some(chunk) = self.chunks.get(&c) {
            add_counts(chunk, &mut counts);
        }
        counts[TerrainMaterial::Air as usize] = 0;
        counts
    }

    /// Make chunks `chunks` hold what `from`'s do, sharing their voxels.
    pub fn copy_chunks(&mut self, from: &Grid, chunks: impl IntoIterator<Item = IVec3>) {
        for c in chunks {
            match from.chunks.get(&c) {
                Some(chunk) => {
                    self.chunks.insert(c, chunk.clone());
                }
                None => {
                    self.chunks.remove(&c);
                }
            }
        }
    }

    /// The chunks holding the voxels in `bounds`, as a grid of their own
    /// (sharing their voxels): what an edit there sends.
    pub fn region(&self, bounds: Bounds) -> Grid {
        let mut out = Grid::new();
        out.copy_chunks(self, bounds.chunks(0));
        out
    }

    /// Take the chunks holding the voxels in `bounds` from `region` (made
    /// by [`Grid::region`]).
    pub fn put_region(&mut self, region: &Grid, bounds: Bounds) {
        self.copy_chunks(region, bounds.chunks(0));
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
                    // Shared voxels are equal without reading them.
                    if Arc::ptr_eq(a, b) || a == b {
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
}

/// The box of a chunk's voxels (from `base`) for which `differs(index)`.
pub(super) fn differing(base: IVec3, differs: impl Fn(usize) -> bool) -> Option<Bounds> {
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

pub(super) fn add_counts(chunk: &Chunk, counts: &mut [u64; 23]) {
    match chunk {
        Chunk::Uniform(v) => counts[v.material as usize] += CHUNK_LEN as u64,
        Chunk::Dense(voxels) => {
            for v in voxels.iter() {
                counts[v.material as usize] += 1;
            }
        }
    }
}
