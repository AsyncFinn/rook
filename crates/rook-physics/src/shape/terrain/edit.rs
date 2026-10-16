//! Scripts' edits to the voxels (the Terrain API in `rook-lua`). The
//! physics world holds the one decoded copy of Workspace's Terrain on
//! each DataModel, so scripts read and edit that copy at once. Collision
//! catches up at the end of the frame ([`Physics::write_terrain`]), as in
//! Studio, where rays find a fill 2 frames later and miss a carved hole's
//! old surface 1 frame later: the colliders of the chunks the frame's
//! edits touched come down, to be built again when something comes near,
//! what rested there wakes, and the voxels go back into `SmoothGrid`,
//! where the renderer and replication see them.

use rook_terrain::MESH_REACH;
use rook_terrain::grid::edit::Bounds;

use super::*;

impl Physics {
    /// The voxels of `terrain`, if it is Workspace's Terrain.
    pub fn terrain_grid(&mut self, dom: &mut Dom, terrain: InstanceId) -> Option<&Grid> {
        self.sync(dom);
        (self.terrain.instance == Some(terrain)).then_some(&self.terrain.grid)
    }

    /// Edit the voxels of `terrain`, Workspace's Terrain: `edit` returns
    /// the voxels it may have changed. Returns `None`, without calling
    /// `edit`, for another Terrain.
    pub fn edit_terrain<R>(
        &mut self,
        dom: &mut Dom,
        terrain: InstanceId,
        edit: impl FnOnce(&mut Grid) -> (R, Option<Bounds>),
    ) -> Option<R> {
        self.sync(dom);
        if self.terrain.instance != Some(terrain) {
            return None;
        }
        let (out, changed) = edit(&mut self.terrain.grid);
        self.terrain.edits.extend(changed);
        Some(out)
    }

    /// The end of a frame: bring collision up to the voxels scripts edited
    /// and write them to the Terrain's `SmoothGrid`.
    pub fn write_terrain(&mut self, dom: &mut Dom) {
        if self.terrain.edits.is_empty() {
            return;
        }
        profiling::scope!("Physics::write_terrain");
        self.sync(dom);
        for bounds in std::mem::take(&mut self.terrain.edits) {
            self.voxels_changed(bounds);
        }
        let Some(terrain) = self.terrain.instance else {
            return;
        };
        let bytes = self.terrain.grid.encode();
        let _ = dom.set_property_by_name(terrain, "SmoothGrid", Value::BinaryString(bytes.into()));
        // The voxels written are the ones here already.
        self.cursor = dom.journal().end();
    }

    /// Take down the colliders of every chunk whose surface the voxels in
    /// `bounds` shape, and wake the bodies near them.
    pub(super) fn voxels_changed(&mut self, bounds: Bounds) {
        for c in bounds.chunks(MESH_REACH) {
            for h in self.terrain.chunk_pieces.remove(&c).unwrap_or_default() {
                self.world.remove_collider(h);
                self.terrain.pieces.remove(&h);
                if let Some(part) = self.colliders.remove(&h) {
                    self.touch.retire(h, part);
                }
            }
            self.terrain.mark_unbuilt(c);
        }
        // Bodies resting on carved terrain fall (Inference).
        let lo = (bounds.min - 1).as_vec3() * VOXEL_STUDS;
        let hi = (bounds.max + 2).as_vec3() * VOXEL_STUDS;
        let wake: Vec<RigidBodyHandle> = self
            .world
            .bodies
            .iter()
            .filter(|(_, rb)| rb.is_dynamic() && rb.is_sleeping())
            .filter(|(_, rb)| {
                rb.colliders().iter().any(|c| {
                    let aabb = self.world.colliders[*c].compute_aabb();
                    let (min, max) = (
                        crate::convert::to_glam(aabb.mins),
                        crate::convert::to_glam(aabb.maxs),
                    );
                    min.cmple(hi).all() && max.cmpge(lo).all()
                })
            })
            .map(|(h, _)| h)
            .collect();
        for h in wake {
            if let Some(rb) = self.world.bodies.get_mut(h) {
                rb.wake_up(true);
            }
        }
    }
}
