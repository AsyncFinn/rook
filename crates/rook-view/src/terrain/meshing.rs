//! New voxels: the levels of detail made from them, the chunks the
//! rings draw meshed (all at once when the place loads, only those the
//! change reached at each level after a script's edit), and each chunk's
//! surfaces handed to the scene as meshes.

use rook_terrain::MESH_REACH;
use rook_terrain::lod::{pyramid, update_pyramid};

use super::*;

impl SceneCache {
    /// Take `grid` as the terrain's voxels, and mesh what the rings around
    /// the camera draw of it that changed (all of it the first time).
    pub(super) fn mesh_terrain(&mut self, grid: Grid) {
        profiling::scope!("mesh_terrain");
        // The voxels change in place once the mesher has let go of them;
        // what it was meshing is of the old ones.
        self.terrain.background.wait();
        let t = &mut self.terrain;
        t.epoch += 1;
        let mut reached: Vec<HashSet<IVec3>> = Vec::new();
        if t.levels.is_empty() {
            t.levels = Arc::new(pyramid(grid));
        } else {
            let levels = Arc::get_mut(&mut t.levels).expect("the mesher let go of the voxels");
            let changes = levels[0].changes(&grid);
            levels[0] = grid;
            reached = update_pyramid(levels, &changes)
                .into_iter()
                .map(|level| level.iter().flat_map(|b| b.chunks(MESH_REACH)).collect())
                .collect();
        }
        let fine = &t.levels[0];
        let counts = fine.counts();
        t.materials = TerrainMaterial::ALL
            .into_iter()
            .filter(|m| counts[*m as usize] > 0)
            .collect();
        let rows: Vec<i32> = fine.chunks().map(|c| c.y).collect();
        // The chunks just below stored ones hold surfaces too.
        t.rows = (
            rows.iter().min().map_or(0, |y| y - 1),
            rows.iter().max().copied().unwrap_or(0),
        );
        for (level, chunks) in reached.iter().enumerate() {
            for c in chunks {
                self.drop_chunk((level as u8, *c));
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
        // The rebuilt scene places every chunk.
        self.install(&job, meshes, false);
    }

    /// Put a chunk's surfaces in the scene's meshes.
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
