//! New voxels: the levels of detail made from them, the chunks the
//! rings draw meshed (all at once when the place loads, only those the
//! change reached at each level after a script's edit), and each chunk's
//! surfaces handed to the scene as meshes.

use rook_terrain::MESH_REACH;
use rook_terrain::lod::{pyramid, update_pyramid};

use super::*;

impl SceneCache {
    /// Take `voxels` as the terrain's voxels, and mesh what the rings
    /// around the camera draw of what changed: the chunks `edits` reach,
    /// or, with `None`, what differs from the voxels meshed before (all of
    /// it the first time). With `place`, the chunks are swapped in the
    /// scene as they are, and the entries that changed are returned;
    /// without, the scene is being rebuilt and places them itself.
    pub(super) fn mesh_terrain(
        &mut self,
        voxels: &Grid,
        edits: Option<&[Bounds]>,
        place: bool,
    ) -> Vec<usize> {
        profiling::scope!("mesh_terrain");
        // The voxels change in place once the mesher has let go of them;
        // what it was meshing is of the old ones.
        self.terrain.background.wait();
        let t = &mut self.terrain;
        t.epoch += 1;
        let mut reached: Vec<FxHashSet<IVec3>> = Vec::new();
        if t.levels.is_empty() {
            // Level 0 shares the DataModel's voxels until they change.
            t.levels = Arc::new(pyramid(voxels.clone()));
            t.counts = voxels.counts();
            t.rows = rows(voxels);
        } else {
            let levels = Arc::get_mut(&mut t.levels).expect("the mesher let go of the voxels");
            let changes = match edits {
                None => {
                    let changes = levels[0].changes(voxels);
                    levels[0] = voxels.clone();
                    t.counts = voxels.counts();
                    t.rows = rows(voxels);
                    changes
                }
                Some(edits) => {
                    let mut chunks: Vec<IVec3> = edits.iter().flat_map(|b| b.chunks(0)).collect();
                    chunks.sort_unstable_by_key(|c| (c.x, c.y, c.z));
                    chunks.dedup();
                    for c in chunks {
                        let old = levels[0].chunk_counts(c);
                        levels[0].copy_chunks(voxels, [c]);
                        let new = levels[0].chunk_counts(c);
                        for ((count, o), n) in t.counts.iter_mut().zip(old).zip(new) {
                            *count = *count - o + n;
                        }
                        // Rows only widen: a row left empty draws nothing.
                        if new.iter().any(|&n| n > 0) {
                            t.rows = (t.rows.0.min(c.y - 1), t.rows.1.max(c.y));
                        }
                    }
                    edits.to_vec()
                }
            };
            reached = update_pyramid(levels, &changes)
                .into_iter()
                .map(|level| level.iter().flat_map(|b| b.chunks(MESH_REACH)).collect())
                .collect();
        }
        let counts = t.counts;
        t.materials = TerrainMaterial::ALL
            .into_iter()
            .filter(|m| counts[*m as usize] > 0)
            .collect();
        let mut changed = Vec::new();
        for (level, chunks) in reached.iter().enumerate() {
            for c in chunks {
                changed.extend(self.drop_chunk((level as u8, *c)));
            }
        }
        let focus = self.focus;
        let target = *self
            .terrain
            .target
            .get_or_insert_with(|| Rings::around(focus));
        let job = self.plan(target);
        let todo = self.todo(&job);
        let meshes = super::rings::mesh_in_place(&self.terrain.levels, &todo);
        changed.extend(self.install(&job, meshes, place));
        changed
    }

    pub(super) fn add_chunk_meshes(&mut self, chunk: ChunkMesh) -> MeshedChunk {
        let origin = chunk.origin();
        let mut surfaces = Vec::new();
        for draw in chunk.draws {
            self.terrain.serial += 1;
            let mut key = blake3::Hasher::new();
            key.update(b"terrain");
            key.update(&self.terrain.serial.to_le_bytes());
            let mesh = MeshHash(*key.finalize().as_bytes());
            let count = draw.positions.len();
            let material = draw.layer.material;
            let terrain_corners = draw
                .corners
                .iter()
                .map(|c| TerrainCorner {
                    corner: c.corner,
                    drawn: (0..3)
                        .filter(|&k| c.layers[k] == draw.layer)
                        .fold(0, |bits, k| bits | 1 << k),
                })
                .collect();
            self.scene.meshes.insert(
                mesh,
                Arc::new(Mesh {
                    positions: draw.positions,
                    normals: draw.normals,
                    uvs: vec![[0.0; 2]; count],
                    indices: draw.indices,
                    terrain: Some(if material == TerrainMaterial::Water {
                        TerrainMapping::Water
                    } else if material.look().cube {
                        TerrainMapping::Cube
                    } else {
                        TerrainMapping::Planes
                    }),
                    terrain_corners,
                    ..Mesh::default()
                }),
            );
            surfaces.push(TerrainSurface {
                mesh,
                origin,
                layer: draw.layer,
                seam: draw.seam,
            });
        }
        MeshedChunk {
            surfaces,
            entries: Vec::new(),
        }
    }
}

/// The lowest and highest level-0 chunk row holding voxels, the lowest
/// one row down: the chunks just below stored ones hold surfaces too.
fn rows(voxels: &Grid) -> (i32, i32) {
    let mut rows: Option<(i32, i32)> = None;
    for c in voxels.chunks() {
        rows = Some(rows.map_or((c.y, c.y), |(lo, hi)| (lo.min(c.y), hi.max(c.y))));
    }
    rows.map_or((0, 0), |(lo, hi)| (lo - 1, hi))
}
