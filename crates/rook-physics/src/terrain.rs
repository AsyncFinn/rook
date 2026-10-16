//! Terrain's collision surface: Workspace's Terrain collides and is
//! queried as the surface `rook-terrain` meshes from its voxels, the one
//! it draws (observed in Studio 0.742: a ball comes to rest on a lone
//! voxel's smooth top, 1.333 studs above its centre, not on its 4-stud
//! box; see `docs/behavior/terrain.md`).
//!
//! Each chunk's surface becomes one triangle-mesh collider per material
//! on a fixed body, so a hit's material is its collider's, and parts
//! slide with that material's friction. A chunk's colliders are built the
//! first time a moving part or a query comes near it ([`near`]): a large
//! map's whole surface (an 86k-part place's 12 million triangles) took 3.5 seconds and
//! gigabytes to build, and a game only ever touches a little of it. Water's surface is a sensor: rays
//! and casts hit it unless they ignore water, and parts touch it, but
//! nothing rests on it. The colliders map to the Terrain instance like a
//! part's, so filters, collision groups and Touched treat it as a part.

use rapier3d::parry::query::RayCullingMode;
use rapier3d::parry::shape::TriMesh;
use rapier3d::prelude::*;
use rapier3d::rayon::prelude::*;
use rook_dom::{Dom, InstanceId};
use rook_terrain::{
    CHUNK_VOXELS, ChunkMesh, Grid, TerrainMaterial, VOXEL_STUDS, chunk_of, chunks_to_mesh,
    mesh_chunk,
};
use rook_types::Value;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

mod near;

use crate::convert::to_rapier;
use crate::{Physics, materials};

/// The terrain in the world.
#[derive(Default)]
pub(crate) struct Terrain {
    /// Workspace's Terrain, which the colliders stand for.
    pub(crate) instance: Option<InstanceId>,
    body: Option<RigidBodyHandle>,
    /// Each collider's material.
    pieces: HashMap<ColliderHandle, TerrainMaterial>,
    /// The voxels, for the queries that test voxels rather than the
    /// surface (`IsRegion3Empty`).
    grid: Grid,
    /// Chunks with a surface whose colliders aren't built yet.
    unbuilt: HashSet<glam::IVec3>,
    /// The first and last chunk with a surface on each axis.
    extent: Option<(glam::IVec3, glam::IVec3)>,
    /// The Terrain's solver and collision groups, for chunks built later.
    pub(crate) groups: (InteractionGroups, InteractionGroups),
    /// The voxels changed (or the Terrain came or went) since the
    /// colliders were built.
    pub(crate) stale: bool,
}

impl Terrain {
    /// The terrain material of a terrain collider; `None` for a part's.
    pub(crate) fn material(&self, collider: ColliderHandle) -> Option<TerrainMaterial> {
        self.pieces.get(&collider).copied()
    }

    pub(crate) fn is_water(&self, collider: ColliderHandle) -> bool {
        self.material(collider) == Some(TerrainMaterial::Water)
    }
}

/// One material's surface in a chunk, as a collider shape.
struct Piece {
    material: TerrainMaterial,
    origin: glam::Vec3,
    mesh: TriMesh,
}

/// A chunk's surface as one mesh per material.
fn pieces(chunk: ChunkMesh) -> Vec<Piece> {
    let origin = chunk.origin();
    let mut by_material: Vec<(TerrainMaterial, Vec<Vector>, Vec<[u32; 3]>)> = Vec::new();
    for s in chunk.surfaces {
        let at = match by_material.iter().position(|(m, ..)| *m == s.material) {
            Some(i) => i,
            None => {
                by_material.push((s.material, Vec::new(), Vec::new()));
                by_material.len() - 1
            }
        };
        let (_, vertices, indices) = &mut by_material[at];
        let first = vertices.len() as u32;
        vertices.extend(s.positions.iter().map(|p| Vector::new(p[0], p[1], p[2])));
        indices.extend(
            s.indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| t.map(|i| first + i)),
        );
    }
    by_material
        .into_iter()
        .filter_map(|(material, vertices, indices)| {
            let mesh = TriMesh::new(vertices, indices).ok()?;
            Some(Piece {
                material,
                origin,
                mesh,
            })
        })
        .collect()
}

/// Workspace's Terrain and its `SmoothGrid` bytes.
fn smooth_grid(dom: &Dom) -> Option<(InstanceId, Option<&[u8]>)> {
    let ws = dom.find_service("Workspace")?;
    let terrain = dom.find_first_child_of_class(ws, "Terrain")?;
    let bytes = dom
        .get(terrain)?
        .properties()
        .iter()
        .find_map(|(id, v)| match v {
            Value::BinaryString(b) if dom.database().property(*id).name == "SmoothGrid" => {
                Some(AsRef::<[u8]>::as_ref(b))
            }
            _ => None,
        });
    Some((terrain, bytes))
}

impl Physics {
    /// Rebuild the terrain's colliders from Workspace's Terrain, if its
    /// voxels changed.
    pub(crate) fn refresh_terrain(&mut self, dom: &Dom) {
        if !std::mem::take(&mut self.terrain.stale) {
            return;
        }
        profiling::scope!("Physics::refresh_terrain");
        if let Some(body) = self.terrain.body.take() {
            self.world.remove_body(body);
            for (collider, _) in self.terrain.pieces.drain() {
                if let Some(part) = self.colliders.remove(&collider) {
                    self.touch.retire(collider, part);
                }
            }
        }
        self.terrain.grid = Grid::new();
        self.terrain.unbuilt.clear();
        self.terrain.extent = None;
        let Some((terrain, bytes)) = smooth_grid(dom) else {
            self.terrain.instance = None;
            return;
        };
        self.terrain.instance = Some(terrain);
        let Some(bytes) = bytes else {
            return;
        };
        self.terrain.grid = match Grid::decode(bytes) {
            Ok(grid) => grid,
            // The renderer warns about it already.
            Err(_) => return,
        };
        self.terrain.unbuilt = chunks_to_mesh(&self.terrain.grid).into_iter().collect();
        self.terrain.extent = self.terrain.unbuilt.iter().fold(None, |e, c| {
            let (lo, hi) = e.unwrap_or((*c, *c));
            Some((lo.min(*c), hi.max(*c)))
        });
        self.terrain.body = Some(self.world.insert_body(RigidBodyBuilder::fixed()));
        self.terrain.groups = self.interaction_groups(dom, terrain);
    }

    /// Build the colliders of these chunks (unbuilt ones, each once).
    fn build_chunks(&mut self, chunks: Vec<glam::IVec3>) {
        let (Some(terrain), Some(body)) = (self.terrain.instance, self.terrain.body) else {
            return;
        };
        profiling::scope!(
            "Physics::build_terrain",
            &format!("{} chunks", chunks.len())
        );
        for c in &chunks {
            self.terrain.unbuilt.remove(c);
        }
        let grid = &self.terrain.grid;
        let pieces: Vec<Piece> = chunks
            .into_par_iter()
            .flat_map_iter(|c| pieces(mesh_chunk(grid, c)))
            .collect();
        let (solver, collision) = self.terrain.groups;
        let params = self.world.integration_parameters;
        for piece in pieces {
            let water = piece.material == TerrainMaterial::Water;
            let surface = materials::of_material(piece.material.material() as u32);
            let collider = ColliderBuilder::new(SharedShape::new(piece.mesh))
                .position(Pose::from_translation(to_rapier(piece.origin)))
                .friction(surface.friction)
                .restitution(surface.elasticity)
                .user_data(crate::contacts::pack(&surface))
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .solver_groups(solver)
                .collision_groups(collision)
                .sensor(water);
            let handle = self.world.insert_collider(collider, Some(body));
            self.colliders.insert(handle, terrain);
            self.terrain.pieces.insert(handle, piece.material);
            // Queries see it before the next step (see `prepare`).
            let co = &self.world.colliders[handle];
            let aabb = co.compute_broad_phase_aabb(&params, &self.world.bodies);
            self.world.broad_phase.set_aabb(&params, handle, aabb);
            self.terrain_placed = true;
        }
    }

    /// A ray's hit on a terrain collider's outside: rays don't hit the
    /// surface from inside (observed: a ray starting inside terrain hits
    /// nothing until it leaves it, then finds the next surface it meets).
    pub(crate) fn terrain_ray(
        &self,
        collider: ColliderHandle,
        ray: &Ray,
        max: f32,
    ) -> Option<RayIntersection> {
        let co = self.world.colliders.get(collider)?;
        co.shape().as_trimesh()?.cast_ray_with_culling(
            co.position(),
            ray,
            max,
            RayCullingMode::IgnoreBackfaces,
        )
    }

    /// Whether `point` is inside solid terrain: the nearest surface above
    /// it faces up, so the point is under it.
    pub(crate) fn inside_terrain(&self, point: glam::Vec3) -> bool {
        let ray = Ray::new(to_rapier(point), Vector::Y);
        let pipeline = self.world.query_pipeline();
        let nearest = pipeline
            .intersect_ray(ray, crate::query::MAX_RAY_LENGTH, false)
            .filter(|(h, ..)| self.terrain.material(*h).is_some() && !self.terrain.is_water(*h))
            .min_by(|a, b| a.2.time_of_impact.total_cmp(&b.2.time_of_impact));
        nearest.is_some_and(|(_, co, hit)| {
            co.shape()
                .as_trimesh()
                .is_some_and(|m| m.triangle(hit.subshape).scaled_normal().y > 0.0)
        })
    }

    /// Whether any voxel (water included) overlaps the box from `min` to
    /// `max` by more than a touch, whatever the surface does inside it:
    /// `IsRegion3Empty` (observed: a box overlapping a solid voxel's corner
    /// by 0.01 studs isn't empty, though the voxel's surface is 1.3 studs
    /// away; one touching a voxel's face is).
    pub fn terrain_voxels_in(&mut self, dom: &mut Dom, min: glam::Vec3, max: glam::Vec3) -> bool {
        self.sync(dom);
        let first = (min / VOXEL_STUDS).floor().as_ivec3();
        let last = (max / VOXEL_STUDS).ceil().as_ivec3() - glam::IVec3::ONE;
        if first.cmpgt(last).any() {
            return false;
        }
        let grid = &self.terrain.grid;
        let (c0, _) = chunk_of(first);
        let (c1, _) = chunk_of(last);
        let mut voxels = Vec::new();
        grid.chunks().any(|c| {
            if c.cmplt(c0).any() || c.cmpgt(c1).any() {
                return false;
            }
            if let Some(v) = grid.uniform(c) {
                return v.material != TerrainMaterial::Air;
            }
            let lo = first.max(c * CHUNK_VOXELS);
            let hi = last.min(c * CHUNK_VOXELS + glam::IVec3::splat(CHUNK_VOXELS - 1));
            grid.read_block(lo, hi - lo + glam::IVec3::ONE, &mut voxels);
            voxels.iter().any(|v| v.material != TerrainMaterial::Air)
        })
    }
}

#[cfg(test)]
mod tests;
