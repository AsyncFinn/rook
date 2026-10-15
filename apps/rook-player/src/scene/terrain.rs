//! Workspace's Terrain: its `SmoothGrid` voxels meshed by `rook_terrain`
//! and drawn in a flat colour per material, each material's colour from
//! `Terrain.MaterialColors` (textured terrain is M6 step 2). Water is drawn
//! in `WaterColor` at `WaterTransparency`.

use rayon::prelude::*;
use rook_content::mesh::Mesh;
use rook_terrain::{Grid, TerrainMaterial, chunks_to_mesh, mesh_chunk};

use super::materials::material_set;
use super::*;

/// The terrain's surfaces, kept until its `SmoothGrid` changes.
#[derive(Debug, Default)]
pub(super) struct TerrainCache {
    /// The hash of the `SmoothGrid` the meshes were built from.
    source: Option<blake3::Hash>,
    /// One entry per chunk and material, in the scene's meshes.
    entries: Vec<PartInstance>,
}

impl SceneCache {
    /// How many scene entries draw the terrain.
    pub fn terrain_meshes(&self) -> usize {
        self.terrain.entries.len()
    }

    /// Add the terrain's entries to the scene, meshing it first if its
    /// voxels aren't the ones meshed last.
    pub(super) fn add_terrain(&mut self, dom: &Dom) {
        profiling::scope!("SceneCache::add_terrain");
        let terrain = dom
            .find_service("Workspace")
            .and_then(|ws| dom.find_first_child_of_class(ws, "Terrain"));
        // Borrowed, not cloned: a large map's grid is megabytes.
        let db = rook_reflection::database();
        let grid = terrain.and_then(|t| {
            dom.get(t)?.properties().iter().find_map(|(id, v)| match v {
                Value::BinaryString(b) if db.property(*id).name == "SmoothGrid" => {
                    Some(AsRef::<[u8]>::as_ref(b))
                }
                _ => None,
            })
        });
        let hash = grid.map(blake3::hash);
        if hash != self.terrain.source {
            self.terrain = TerrainCache {
                source: hash,
                entries: match (terrain, grid) {
                    (Some(t), Some(bytes)) => self.mesh_terrain(dom, t, bytes),
                    _ => Vec::new(),
                },
            };
        }
        for entry in &self.terrain.entries {
            self.scene.parts.push(*entry);
            self.poses.push(Pose::default());
        }
    }

    fn mesh_terrain(&mut self, dom: &Dom, terrain: InstanceId, bytes: &[u8]) -> Vec<PartInstance> {
        profiling::scope!("mesh_terrain");
        let grid = match Grid::decode(bytes) {
            Ok(grid) => grid,
            Err(e) => {
                eprintln!("warning: Terrain.SmoothGrid doesn't decode: {e}");
                return Vec::new();
            }
        };
        let colors = match dom.property_by_name(terrain, "MaterialColors") {
            Some(Value::MaterialColors(c)) => Some(c),
            _ => None,
        };
        let water =
            color(dom, terrain, "WaterColor").unwrap_or([12.0 / 255.0, 84.0 / 255.0, 92.0 / 255.0]);
        let water_transparency = number(dom, terrain, "WaterTransparency").unwrap_or(0.3);
        let set = material_set(dom);
        let source = blake3::hash(bytes);
        let meshes: Vec<_> = chunks_to_mesh(&grid)
            .par_iter()
            .map(|c| mesh_chunk(&grid, *c))
            .collect();
        let mut entries = Vec::new();
        for chunk in meshes {
            let origin = chunk.origin();
            for surface in chunk.surfaces {
                let mut key = blake3::Hasher::new();
                key.update(source.as_bytes());
                for v in chunk.chunk.to_array() {
                    key.update(&v.to_le_bytes());
                }
                key.update(&[surface.material as u8]);
                let hash = MeshHash(*key.finalize().as_bytes());
                let count = surface.positions.len();
                self.scene.meshes.insert(
                    hash,
                    Arc::new(Mesh {
                        positions: surface.positions,
                        normals: surface.normals,
                        uvs: vec![[0.0; 2]; count],
                        indices: surface.indices,
                        ..Mesh::default()
                    }),
                );
                let mut part =
                    PartInstance::plain(DrawShape::File(hash), origin, Mat3::IDENTITY, Vec3::ONE);
                part.material = surface.material.material();
                part.material_set = set;
                if surface.material == TerrainMaterial::Water {
                    part.color = water;
                    part.transparency = water_transparency;
                    part.cast_shadow = false;
                } else if let Some(c) = surface.material.color(colors.as_ref()) {
                    part.color = [c.r, c.g, c.b].map(|v| f32::from(v) / 255.0);
                }
                entries.push(part);
            }
        }
        entries
    }
}
