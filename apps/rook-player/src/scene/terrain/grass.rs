//! Grass blades near the camera: with `Terrain.Decoration` on, each tile
//! of Grass tops (`rook_terrain::grass`) within [`GRASS_REACH`] of the
//! camera gets an entry drawing its blades, built when the camera comes
//! near and dropped, mesh and all, when it leaves.

use glam::{IVec2, Vec2};
use rook_terrain::grass::{GrassTops, TILE_STUDS, blades, tile_of, tile_origin};

use super::*;

/// How far from the camera blades are drawn, in studs (observed: Studio
/// still draws some 200 studs away).
pub(super) const GRASS_REACH: f32 = 200.0;

/// The blades drawn: each tile's entry and mesh.
#[derive(Debug, Default)]
pub(super) struct Grass {
    /// Grass's top triangles, by tile.
    pub(super) tops: GrassTops,
    /// `Terrain.Decoration`.
    pub(super) on: bool,
    /// The blades' colour (`rook_terrain::grass::blade_color`).
    pub(super) color: [f32; 3],
    shown: HashMap<IVec2, (usize, MeshHash)>,
    /// Meshes of tiles no longer drawn, for the renderer to free.
    dropped: Vec<MeshHash>,
}

impl Grass {
    /// The entries drawing blades.
    pub(super) fn entries(&self) -> Vec<usize> {
        self.shown.values().map(|(i, _)| *i).collect()
    }

    /// Forget the tiles drawn (the scene's entries were rebuilt): their
    /// meshes, for the scene and the renderer to drop.
    pub(super) fn reset(&mut self) -> Vec<MeshHash> {
        let meshes: Vec<_> = self.shown.drain().map(|(_, (_, mesh))| mesh).collect();
        self.dropped.extend(&meshes);
        meshes
    }
}

/// Whether some of `tile` is within `reach` of `camera`, across X and Z.
fn near(tile: IVec2, camera: Vec3, reach: f32) -> bool {
    let corner = tile_origin(tile);
    let lo = Vec2::new(corner.x, corner.z);
    let c = Vec2::new(camera.x, camera.z);
    c.clamp(lo, lo + Vec2::splat(TILE_STUDS)).distance(c) < reach
}

impl SceneCache {
    /// Draw the blades of the tiles near `camera` and stop drawing the
    /// rest. Returns the entries that changed.
    pub(crate) fn update_grass(&mut self, camera: Vec3) -> Vec<usize> {
        let mut changed = Vec::new();
        let reach = if self.terrain.grass.on {
            GRASS_REACH
        } else {
            0.0
        };
        let grass = &mut self.terrain.grass;
        let gone: Vec<IVec2> = grass
            .shown
            .keys()
            .filter(|t| !near(**t, camera, reach))
            .copied()
            .collect();
        let mut released = Vec::new();
        for tile in gone {
            let (i, mesh) = grass.shown.remove(&tile).expect("shown");
            grass.dropped.push(mesh);
            released.push((i, mesh));
        }
        for (i, mesh) in released {
            self.scene.meshes.remove(&mesh);
            self.release(i);
            changed.push(i);
        }
        if reach == 0.0 {
            return changed;
        }
        let r = (reach / TILE_STUDS).ceil() as i32 + 1;
        let centre = tile_of(camera);
        for z in -r..=r {
            for x in -r..=r {
                let tile = centre + IVec2::new(x, z);
                let grass = &self.terrain.grass;
                if grass.shown.contains_key(&tile) || !near(tile, camera, reach) {
                    continue;
                }
                let Some(triangles) = grass.tops.get(&tile) else {
                    continue;
                };
                let b = blades(tile, triangles);
                let mut key = blake3::Hasher::new();
                key.update(b"grass");
                if let Some(source) = self.terrain.source {
                    key.update(source.as_bytes());
                }
                key.update(&tile.x.to_le_bytes());
                key.update(&tile.y.to_le_bytes());
                let mesh = MeshHash(*key.finalize().as_bytes());
                self.scene.meshes.insert(
                    mesh,
                    Arc::new(Mesh {
                        positions: b.positions,
                        normals: b.normals,
                        uvs: b.uvs,
                        indices: b.indices,
                        terrain: Some(TerrainMapping::Blades),
                        ..Mesh::default()
                    }),
                );
                let mut part = PartInstance::plain(
                    DrawShape::File(mesh),
                    tile_origin(tile),
                    Mat3::IDENTITY,
                    Vec3::ONE,
                );
                part.material = Material::Grass;
                part.material_set = MaterialSet::Materials2022;
                part.color = grass.color;
                part.cast_shadow = false;
                let i = self.take_index(part, Pose::default());
                self.terrain.grass.shown.insert(tile, (i, mesh));
                changed.push(i);
            }
        }
        changed
    }

    /// The meshes of grass tiles no longer drawn, for
    /// [`Renderer::drop_mesh`] once the renderer has their entries hidden.
    pub(crate) fn take_dropped_meshes(&mut self) -> Vec<MeshHash> {
        std::mem::take(&mut self.terrain.grass.dropped)
    }
}
