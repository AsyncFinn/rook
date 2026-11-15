//! Scene queries: rays, shape casts, regions, overlaps and touching parts,
//! answered by Rapier's query pipeline over the same colliders the
//! simulation uses. Each query first brings the world up to date with the
//! DOM, as scripts query what they just made or moved. The rules are in
//! `docs/behavior/physics/queries.md`.

use rustc_hash::FxHashSet;

use glam::Vec3;
use rapier3d::parry::query::{self as parry, ShapeCastOptions};
use rapier3d::prelude::*;
use rook_dom::{Dom, InstanceId};
use rook_types::CFrame;

use crate::contacts::groups;
use crate::convert::pose_of;
use crate::{Physics, shape};

pub(crate) mod bounds;
mod casts;
mod filter;
mod overlaps;
mod rising;
pub(crate) mod world_models;

use casts::surface_normal;
use filter::Accepting;
pub use filter::Filter;
use overlaps::OVERLAP;

/// Rays find nothing further than this many studs from their origin
/// (observed for `FindPartOnRay` and `Raycast`, whatever the ray's
/// length); a missed legacy ray still reports its full end point.
pub const MAX_RAY_LENGTH: f32 = 15_000.0;

/// `Spherecast`'s largest radius, `Blockcast`'s largest side and the
/// longest distance either sweeps (observed; larger values raise errors).
pub const MAX_CAST_RADIUS: f32 = 256.0;
pub const MAX_CAST_SIDE: f32 = 512.0;
pub const MAX_CAST_DISTANCE: f32 = 1024.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub part: InstanceId,
    pub position: Vec3,
    /// Out of the surface hit.
    pub normal: Vec3,
    /// How far the ray (or the swept shape) traveled.
    pub distance: f32,
    /// The part's `Enum.Material` value.
    pub material: u32,
}

/// The space a bounds query covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Region {
    /// A box of `size` at `cframe` (Region3: axis-aligned).
    Box {
        cframe: CFrame,
        size: Vec3,
    },
    Sphere {
        center: Vec3,
        radius: f32,
    },
}

impl Region {
    fn shape(&self) -> (SharedShape, Pose) {
        match *self {
            Region::Box { cframe, size } => {
                let h = size.abs() * 0.5;
                (SharedShape::cuboid(h.x, h.y, h.z), pose_of(&cframe))
            }
            Region::Sphere { center, radius } => (
                SharedShape::ball(radius.max(0.0)),
                pose_of(&CFrame::from_position(center)),
            ),
        }
    }
}

/// The shape a `Blockcast` or `Spherecast` sweeps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CastShape {
    Sphere(f32),
    Block(Vec3),
}

impl CastShape {
    fn shape(self) -> SharedShape {
        match self {
            CastShape::Sphere(r) => SharedShape::ball(r.max(shape::MIN_PART_SIZE)),
            CastShape::Block(size) => {
                let h = size.max(Vec3::splat(shape::MIN_PART_SIZE)) * 0.5;
                SharedShape::cuboid(h.x, h.y, h.z)
            }
        }
    }
}

impl Physics {
    /// Bring the world up to the current DOM for a query (see the module
    /// docs). The first call builds the whole world and its query tree, so
    /// a DataModel makes it while loading rather than at its first query.
    pub fn prepare(&mut self, dom: &mut Dom) {
        profiling::scope!("Physics::prepare_query");
        self.sync(dom);
        let terrain_placed = std::mem::take(&mut self.terrain_placed);
        if self.stale.is_empty() && self.stale_statics.is_empty() && !terrain_placed {
            return;
        }
        let params = self.world.integration_parameters;
        let place = |world: &mut PhysicsWorld, pose: Pose, collider: ColliderHandle| {
            if let Some(co) = world.colliders.get_mut(collider) {
                let local = co.position_wrt_parent().copied().unwrap_or_default();
                co.set_position(pose * local);
                let aabb = co.compute_broad_phase_aabb(&params, &world.bodies);
                world.broad_phase.set_aabb(&params, collider, aabb);
            }
        };
        for h in std::mem::take(&mut self.stale) {
            let Some(body) = self.bodies.get(&h) else {
                continue;
            };
            let pose = *self.world.bodies[h].position();
            for (collider, _) in body.colliders.iter().flatten() {
                place(&mut self.world, pose, *collider);
            }
        }
        // A static's collider is already in place: only its box moves.
        for part in std::mem::take(&mut self.stale_statics) {
            if let Some((collider, _)) = self.statics.get(&part).and_then(|s| s.collider)
                && let Some(co) = self.world.colliders.get(collider)
            {
                let aabb = co.compute_broad_phase_aabb(&params, &self.world.bodies);
                self.world.broad_phase.set_aabb(&params, collider, aabb);
            }
        }
        // The broad phase keeps what `set_aabb` changed until its next
        // update, which a step does. A world that isn't stepping (a
        // client's with no character, or one with nothing unanchored) updates it here
        // so the list doesn't grow. The pairs it reports are dropped:
        // fixed bodies don't touch each other, and parts that start
        // simulating get new colliders, whose pairs are reported then.
        if !self.moves_anything() || self.playback.only {
            let mut pairs = Vec::new();
            self.world.broad_phase.update(
                &params,
                &self.world.colliders,
                &self.world.bodies,
                &[],
                &[],
                &mut pairs,
            );
        }
    }

    /// The part of each collider `filter` accepts.
    fn part_filter<'a>(
        &'a self,
        dom: &'a Dom,
        filter: &'a Filter,
    ) -> impl Fn(ColliderHandle) -> Option<InstanceId> + 'a {
        let accepting = self.accepting(dom, filter);
        move |h| {
            let part = *self.colliders.get(&h)?;
            if filter.ignore_water && self.terrain.is_water(h) {
                return None;
            }
            accepting.as_ref()?.accepts(part).then_some(part)
        }
    }

    /// Whether a query with [`Filter::skip_invisible_ghosts`] passes
    /// through `part`: asked of a ray's hit, it says whether that query
    /// would find anything else.
    pub fn is_invisible_ghost(&self, dom: &Dom, part: InstanceId) -> bool {
        (self.props.as_ref()).is_some_and(|props| filter::is_invisible_ghost(dom, props, part))
    }

    /// `filter` for one query; `None` before the world was first brought
    /// up to the DOM, when it holds no part to accept.
    fn accepting<'a>(&'a self, dom: &'a Dom, filter: &'a Filter) -> Option<Accepting<'a>> {
        let props = self.props.as_ref()?;
        Some(Accepting::new(filter, dom, props, &self.groups))
    }

    /// The `Enum.Material` value a hit on collider `h` of `part` reports:
    /// the part's, or the terrain material the collider is made of.
    pub(crate) fn material_of(&self, dom: &Dom, h: ColliderHandle, part: InstanceId) -> u32 {
        match self.terrain.material(h) {
            Some(m) => m.material() as u32,
            None => dom.enum_property(part, "Material").unwrap_or(0),
        }
    }
}
