//! Terrain's collision surface: Workspace's Terrain collides and is
//! queried as the surface `rook-terrain` meshes from its voxels, one
//! triangle-mesh collider per material and chunk, built when something first
//! comes near the chunk ([`near`]); water's is a sensor. Scripts' edits
//! rebuild only the chunks they touch ([`edit`]). Measurements are in
//! `docs/behavior/terrain/collision.md`.

use rapier3d::parry::query::RayCullingMode;
use rapier3d::parry::shape::TriMesh;
use rapier3d::prelude::*;
use rook_dom::{Dom, InstanceId};
use rook_terrain::{
    CHUNK_VOXELS, ChunkMesh, Grid, TerrainMaterial, VOXEL_STUDS, chunk_of, chunks_to_mesh,
    mesh_chunk,
};
use rook_types::Value;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

mod edit;
mod near;

use crate::Physics;
use crate::contacts::materials;
use crate::convert::to_rapier;

/// The terrain in the world.
#[derive(Default)]
pub(crate) struct Terrain {
    /// Workspace's Terrain, which the colliders stand for.
    pub(crate) instance: Option<InstanceId>,
    body: Option<RigidBodyHandle>,
    /// Each collider's material.
    pieces: HashMap<ColliderHandle, TerrainMaterial>,
    /// Each built chunk's colliders, so an edit can take them down.
    chunk_pieces: HashMap<glam::IVec3, Vec<ColliderHandle>>,
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
    /// The voxels scripts edited this frame (see [`edit`]).
    edits: Vec<rook_terrain::grid::edit::Bounds>,
}

impl Terrain {
    /// Build chunk `c`'s colliders when something comes near it.
    fn mark_unbuilt(&mut self, c: glam::IVec3) {
        self.unbuilt.insert(c);
        let (lo, hi) = self.extent.unwrap_or((c, c));
        self.extent = Some((lo.min(c), hi.max(c)));
    }

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
    chunk: glam::IVec3,
    material: TerrainMaterial,
    origin: glam::Vec3,
    mesh: TriMesh,
}

/// A chunk's surface as one mesh per material.
fn pieces(chunk: ChunkMesh) -> Vec<Piece> {
    let origin = chunk.origin();
    let at = chunk.chunk;
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
                chunk: at,
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
        if self.update_terrain(dom) {
            return;
        }
        if let Some(body) = self.terrain.body.take() {
            self.world.remove_body(body);
            for (collider, _) in self.terrain.pieces.drain() {
                if let Some(part) = self.colliders.remove(&collider) {
                    self.touch.retire(collider, part);
                }
            }
        }
        self.terrain.chunk_pieces.clear();
        self.terrain.grid = Grid::new();
        self.terrain.unbuilt.clear();
        self.terrain.extent = None;
        self.terrain.edits.clear();
        let Some((terrain, bytes)) = smooth_grid(dom) else {
            self.terrain.instance = None;
            return;
        };
        self.terrain.instance = Some(terrain);
        // Even an empty Terrain gets its body: scripts may fill it.
        self.terrain.body = Some(self.world.insert_body(RigidBodyBuilder::fixed()));
        self.terrain.groups = self.interaction_groups(dom, terrain);
        self.terrain.grid = match bytes.map(Grid::decode) {
            Some(Ok(grid)) => grid,
            // The renderer warns about one that doesn't decode.
            None | Some(Err(_)) => return,
        };
        for c in chunks_to_mesh(&self.terrain.grid) {
            self.terrain.mark_unbuilt(c);
        }
    }

    /// Only the voxels of the same Terrain changed (a script's edit
    /// replicated, or written by hand): rebuild just the chunks that
    /// differ. Returns false when the colliders must be built anew.
    fn update_terrain(&mut self, dom: &Dom) -> bool {
        let Some((terrain, bytes)) = smooth_grid(dom) else {
            return false;
        };
        if self.terrain.instance != Some(terrain)
            || self.terrain.body.is_none()
            || self.interaction_groups(dom, terrain) != self.terrain.groups
        {
            return false;
        }
        let grid = match bytes.map(Grid::decode) {
            Some(Ok(grid)) => grid,
            None | Some(Err(_)) => Grid::new(),
        };
        let changes = self.terrain.grid.changes(&grid);
        self.terrain.grid = grid;
        self.terrain.edits.clear();
        for bounds in changes {
            self.voxels_changed(bounds);
        }
        true
    }

    /// Build the colliders of these chunks (unbuilt ones, each once).
    fn build_chunks(&mut self, chunks: &[glam::IVec3]) {
        let (Some(terrain), Some(body)) = (self.terrain.instance, self.terrain.body) else {
            return;
        };
        profiling::scope!(
            "Physics::build_terrain",
            &format!("{} chunks", chunks.len())
        );
        for c in chunks {
            self.terrain.unbuilt.remove(c);
        }
        let grid = &self.terrain.grid;
        let pieces: Vec<Piece> = rook_platform::get()
            .workers
            .map(chunks, |&c| pieces(mesh_chunk(grid, c)))
            .into_iter()
            .flatten()
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
            self.terrain
                .chunk_pieces
                .entry(piece.chunk)
                .or_default()
                .push(handle);
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
