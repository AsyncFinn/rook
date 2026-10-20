//! Meshing the voxels a chunk at a time, and handing each chunk's
//! surfaces to the scene as meshes.

use super::*;

impl SceneCache {
    /// Mesh the chunks of `grid` that differ from the voxels meshed last
    /// (all of them the first time), and the neighbours whose surfaces
    /// reach the voxels that changed.
    pub(super) fn mesh_terrain(&mut self, grid: Grid) {
        profiling::scope!("mesh_terrain");
        let wanted: HashSet<IVec3> = chunks_to_mesh(&grid).into_iter().collect();
        let mut remesh: HashSet<IVec3> = HashSet::new();
        if self.terrain.chunks.is_empty() {
            remesh.extend(&wanted);
        } else {
            for bounds in self.terrain.grid.changes(&grid) {
                remesh.extend(bounds.chunks(rook_terrain::MESH_REACH));
            }
        }
        let terrain = &mut self.terrain;
        let gone: Vec<IVec3> = terrain
            .chunks
            .keys()
            .filter(|c| remesh.contains(c) || !wanted.contains(c))
            .copied()
            .collect();
        for c in gone {
            for s in terrain.chunks.remove(&c).expect("meshed").surfaces {
                self.scene.meshes.remove(&s.mesh);
            }
        }
        terrain.epoch += 1;
        let todo: Vec<IVec3> = remesh.into_iter().filter(|c| wanted.contains(c)).collect();
        let meshes: Vec<ChunkMesh> = todo.par_iter().map(|c| mesh_chunk(&grid, *c)).collect();
        for chunk in meshes {
            let meshed = self.add_chunk_meshes(chunk);
            self.terrain.chunks.insert(meshed.0, meshed.1);
        }
        let terrain = &mut self.terrain;
        terrain.grid = grid;
        let mut order: Vec<&IVec3> = terrain.chunks.keys().collect();
        order.sort_unstable_by_key(|c| (c.x, c.y, c.z));
        terrain.surfaces = order
            .iter()
            .flat_map(|c| terrain.chunks[*c].surfaces.iter().copied())
            .collect();
        let mut tops = GrassTops::default();
        for c in order {
            for (tile, tris) in &terrain.chunks[c].tops {
                tops.entry(*tile).or_default().extend(tris);
            }
        }
        terrain.grass.tops = tops;
    }

    /// Put a chunk's surfaces in the scene's meshes.
    fn add_chunk_meshes(&mut self, chunk: ChunkMesh) -> (IVec3, MeshedChunk) {
        let mut tops = GrassTops::default();
        add_grass_tops(&chunk, &mut tops);
        let origin = chunk.origin();
        let mut surfaces = Vec::new();
        for draw in chunk.draws {
            let mut key = blake3::Hasher::new();
            key.update(&self.terrain.epoch.to_le_bytes());
            let seam = match draw.seam {
                None => 0,
                Some(Seam::First) => 1,
                Some(Seam::Over) => 2,
            };
            key.update(&[draw.layer.index() as u8, seam]);
            for v in chunk.chunk.to_array() {
                key.update(&v.to_le_bytes());
            }
            let mesh = MeshHash(*key.finalize().as_bytes());
            let count = draw.positions.len();
            let material = draw.layer.material;
            let terrain_corners = draw
                .corners
                .iter()
                .map(|c| TerrainCorner {
                    layers: c.layers.map(|l| l.index() as u8),
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
                    } else if material.shape().cube {
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
        (chunk.chunk, MeshedChunk { surfaces, tops })
    }
}
