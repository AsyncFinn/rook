//! Scene queries: rays, shape casts, regions, overlaps and touching parts,
//! answered by Rapier's query pipeline over the same colliders the
//! simulation uses. Each query first brings the world up to date with the
//! DOM, as scripts query what they just made or moved. The rules are in
//! `docs/behavior/physics.md` (Scene queries).

use rustc_hash::FxHashSet as HashSet;

use glam::Vec3;
use rapier3d::parry::query::{self as parry, ShapeCastOptions};
use rapier3d::prelude::*;
use rook_dom::{Dom, InstanceId};
use rook_types::CFrame;

use crate::contacts::groups;
use crate::convert::{pose_of, to_glam, to_rapier};
use crate::{Physics, shape};

mod filter;

pub use filter::Filter;

/// Rays find nothing further than this many studs from their origin
/// (observed for `FindPartOnRay` and `Raycast`, whatever the ray's
/// length); a missed legacy ray still reports its full end point.
pub const MAX_RAY_LENGTH: f32 = 15_000.0;

/// `Spherecast`'s largest radius, `Blockcast`'s largest side and the
/// longest distance either sweeps (observed; larger values raise errors).
pub const MAX_CAST_RADIUS: f32 = 256.0;
pub const MAX_CAST_SIDE: f32 = 512.0;
pub const MAX_CAST_DISTANCE: f32 = 1024.0;

/// How far shapes must overlap to count as touching, in studs, so faces
/// that only meet (within float error, far from the origin) don't.
/// Inference: Studio counts 0.01 studs of overlap and not faces that meet.
const OVERLAP: f32 = 1e-3;

/// What a ray or shape cast hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub part: InstanceId,
    pub position: Vec3,
    /// Out of the surface hit.
    pub normal: Vec3,
    /// How far the ray (or the swept shape) travelled.
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
    /// A box of this size.
    Block(Vec3),
}

impl Physics {
    /// Bring the world up to the current DOM for a query (see the module
    /// docs).
    pub(crate) fn prepare(&mut self, dom: &mut Dom) {
        profiling::scope!("Physics::prepare_query");
        self.sync(dom);
        let terrain_placed = std::mem::take(&mut self.terrain_placed);
        if self.stale.is_empty() && !terrain_placed {
            return;
        }
        let params = self.world.integration_parameters;
        for h in std::mem::take(&mut self.stale) {
            let Some(body) = self.bodies.get(&h) else {
                continue;
            };
            let pose = *self.world.bodies[h].position();
            for (collider, _) in body.colliders.iter().flatten() {
                if let Some(co) = self.world.colliders.get_mut(*collider) {
                    let local = co.position_wrt_parent().copied().unwrap_or_default();
                    co.set_position(pose * local);
                    let aabb = co.compute_broad_phase_aabb(&params, &self.world.bodies);
                    self.world.broad_phase.set_aabb(&params, *collider, aabb);
                }
            }
        }
        // The broad phase keeps what `set_aabb` changed until its next
        // update, which a step does. A world that isn't stepping (a
        // client's, or a server's with nothing unanchored) updates it here
        // so the list doesn't grow. The pairs it reports are dropped:
        // fixed bodies don't touch each other, and parts that start
        // simulating get new colliders, whose pairs are reported then.
        if self.query_only || self.bodies.values().all(|b| b.asm.anchored) {
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
        move |h| {
            let part = *self.colliders.get(&h)?;
            if filter.ignore_water && self.terrain.is_water(h) {
                return None;
            }
            filter.accepts(dom, &self.groups, part).then_some(part)
        }
    }

    /// The first part along a ray from `origin` covering `direction` (its
    /// length is the maximum distance, up to [`MAX_RAY_LENGTH`]).
    pub fn raycast(
        &mut self,
        dom: &mut Dom,
        origin: Vec3,
        direction: Vec3,
        filter: &Filter,
    ) -> Option<Hit> {
        profiling::scope!("Physics::raycast");
        self.prepare(dom);
        let dom = &*dom;
        let length = direction.length();
        if !length.is_finite() || length <= 0.0 || !origin.is_finite() {
            return None;
        }
        let dir = direction / length;
        let max = length.min(MAX_RAY_LENGTH);
        self.build_terrain_along(origin, dir, max);
        let ray = Ray::new(to_rapier(origin), to_rapier(dir));
        let accept = self.part_filter(dom, filter);
        // Parts: the nearest hit, found nearest first so the search stops
        // there instead of testing every part the ray passes. A part the
        // ray starts inside (or on) would be hit at 0: it's skipped, and
        // the next part along is hit.
        let part_ok = |h: ColliderHandle, co: &Collider| {
            self.terrain.material(h).is_none()
                && accept(h).is_some()
                && !(co.compute_aabb().contains_local_point(ray.origin)
                    && co
                        .shape()
                        .cast_ray(co.position(), &ray, max, true)
                        .is_some_and(|t| t <= 0.0))
        };
        let parts = self
            .world
            .query_pipeline_with_filter(QueryFilter::new().predicate(&part_ok))
            .cast_ray_and_get_normal(&ray, max, true)
            .and_then(|(h, hit)| Some((h, *self.colliders.get(&h)?, hit)));
        // Terrain is a surface, hit only from outside, and only nearer
        // than the part hit.
        let reach = parts.map_or(max, |(.., hit)| hit.time_of_impact);
        let is_terrain = |h: ColliderHandle, _: &Collider| self.terrain.material(h).is_some();
        let terrain = self
            .world
            .query_pipeline_with_filter(QueryFilter::new().predicate(&is_terrain))
            .intersect_ray(ray, reach, true)
            .filter_map(|(h, ..)| {
                let hit = self.terrain_ray(h, &ray, reach)?;
                Some((h, accept(h)?, hit))
            })
            .filter(|(.., hit)| hit.time_of_impact > 0.0)
            .min_by(|a, b| a.2.time_of_impact.total_cmp(&b.2.time_of_impact));
        let (h, part, hit) = terrain.or(parts)?;
        let distance = hit.time_of_impact;
        Some(Hit {
            part,
            position: origin + dir * distance,
            normal: surface_normal(to_glam(hit.normal)),
            distance,
            material: self.material_of(dom, h, part),
        })
    }

    /// The first part a shape at `cframe` hits moving along `direction`
    /// (its length is the distance). Parts the shape starts in are skipped
    /// (observed).
    pub fn shapecast(
        &mut self,
        dom: &mut Dom,
        shape: CastShape,
        cframe: CFrame,
        direction: Vec3,
        filter: &Filter,
    ) -> Option<Hit> {
        profiling::scope!("Physics::shapecast");
        self.prepare(dom);
        let dom = &*dom;
        let length = direction.length();
        if !length.is_finite() || length <= 0.0 {
            return None;
        }
        let cast: SharedShape = match shape {
            CastShape::Sphere(r) => SharedShape::ball(r.max(shape::MIN_PART_SIZE)),
            CastShape::Block(size) => {
                let h = size.max(Vec3::splat(shape::MIN_PART_SIZE)) * 0.5;
                SharedShape::cuboid(h.x, h.y, h.z)
            }
        };
        let start = pose_of(&cframe);
        let from = cast.compute_aabb(&start);
        let to = cast.compute_aabb(&(Pose::from_translation(to_rapier(direction)) * start));
        self.build_terrain_in(
            to_glam(from.mins.min(to.mins)),
            to_glam(from.maxs.max(to.maxs)),
        );
        let accept = self.part_filter(dom, filter);
        let predicate = |h: ColliderHandle, co: &Collider| {
            accept(h).is_some()
                && !parry::intersection_test(&start, &*cast, co.position(), co.shape())
                    .is_ok_and(|t| t.intersecting)
        };
        let pipeline = self
            .world
            .query_pipeline_with_filter(QueryFilter::new().predicate(&predicate));
        let options = ShapeCastOptions {
            max_time_of_impact: length,
            target_distance: 0.0,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        };
        let dir = direction / length;
        let (h, hit) = pipeline.cast_shape(&start, to_rapier(dir), &*cast, options)?;
        let part = *self.colliders.get(&h)?;
        // witness1 and normal1 are on the part hit, in world space.
        Some(Hit {
            part,
            position: to_glam(hit.witness1),
            normal: surface_normal(to_glam(hit.normal1)),
            distance: hit.time_of_impact,
            material: self.material_of(dom, h, part),
        })
    }

    /// The `Enum.Material` value a hit on collider `h` of `part` reports:
    /// the part's, or the terrain material the collider is made of.
    fn material_of(&self, dom: &Dom, h: ColliderHandle, part: InstanceId) -> u32 {
        match self.terrain.material(h) {
            Some(m) => m.material() as u32,
            None => dom.enum_property(part, "Material").unwrap_or(0),
        }
    }

    /// Parts whose collider overlaps `shape` at `pose` by `test`, in id
    /// order (Approximation: Roblox's order is its own; parts made in
    /// order came back in that order), at most `max` of them (0: all).
    pub(crate) fn overlapping(
        &self,
        dom: &Dom,
        query: (&dyn Shape, Pose),
        filter: &Filter,
        max: usize,
        skip: Option<InstanceId>,
        test: impl Fn(InstanceId, &Collider) -> bool,
    ) -> Vec<InstanceId> {
        let (shape, pose) = query;
        let accept = self.part_filter(dom, filter);
        let aabb = shape.compute_aabb(&pose);
        let mut found: Vec<InstanceId> = self
            .world
            .query_pipeline()
            .intersect_aabb_conservative(aabb)
            .filter_map(|(h, co)| {
                if self.terrain.material(h).is_some() {
                    return None;
                }
                let part = accept(h).filter(|p| Some(*p) != skip)?;
                test(part, co).then_some(part)
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        found.sort_unstable();
        if max > 0 {
            found.truncate(max);
        }
        found
    }

    /// Parts whose `Size` box meets `shape` at `pose`: Region3 (an
    /// axis-aligned box), `GetPartBoundsInBox` and `GetPartBoundsInRadius`.
    ///
    /// Approximation: candidates come from the colliders' bounds, so a
    /// cylinder whose box is much wider than its round shape can be missed
    /// near its box's corners.
    pub fn parts_in_bounds(
        &mut self,
        dom: &mut Dom,
        region: Region,
        filter: &Filter,
        max: usize,
    ) -> Vec<InstanceId> {
        profiling::scope!("Physics::parts_in_bounds");
        self.prepare(dom);
        let dom = &*dom;
        let (shape, pose) = region.shape();
        self.overlapping(dom, (&*shape, pose), filter, max, None, |part, _| {
            shape::world_box(dom, part).is_some_and(|(b, at)| {
                parry::intersection_test(&pose, &*shape, &at, &*b).is_ok_and(|t| t.intersecting)
            })
        })
    }

    /// `WorldRoot:GetPartsInPart`: parts whose shape overlaps `part`'s.
    /// The part needn't be in Workspace (observed).
    pub fn parts_in_part(
        &mut self,
        dom: &mut Dom,
        part: InstanceId,
        filter: &Filter,
        max: usize,
    ) -> Vec<InstanceId> {
        profiling::scope!("Physics::parts_in_part");
        self.prepare(dom);
        let Some((shape, pose)) = shape::world_shape(dom, &mut self.unions, part) else {
            return Vec::new();
        };
        self.overlapping(dom, (&*shape, pose), filter, max, Some(part), |_, co| {
            overlaps(&*shape, &pose, co)
        })
    }

    /// `BasePart:GetTouchingParts`: Workspace parts overlapping `part`,
    /// its own assembly's included. A colliding part only reports
    /// colliding parts; one with `CanCollide` off reports nothing unless
    /// `touch_listened` (something is connected to its Touched, which
    /// gives it a TouchInterest), and then every part it overlaps
    /// (observed; creator-docs says the same). `CanQuery` doesn't matter.
    /// Terrain's solid surface counts for every part (observed).
    pub fn touching_parts(
        &mut self,
        dom: &mut Dom,
        part: InstanceId,
        touch_listened: bool,
    ) -> Vec<InstanceId> {
        profiling::scope!("Physics::touching_parts");
        self.prepare(dom);
        if !self.parts.contains_key(&part) {
            return Vec::new();
        }
        let collides = |p| dom.bool_property(p, "CanCollide").unwrap_or(true);
        // Parts that touch it, if any: all of them, or only colliding ones.
        let others = if collides(part) {
            Some(false)
        } else if touch_listened {
            Some(true)
        } else {
            None
        };
        let Some((shape, pose)) = shape::world_shape(dom, &mut self.unions, part) else {
            return Vec::new();
        };
        let aabb = shape.compute_aabb(&pose);
        let center = to_glam(pose.translation);
        self.build_terrain_in(to_glam(aabb.mins), to_glam(aabb.maxs));
        self.build_terrain_along(center, Vec3::Y, MAX_RAY_LENGTH);
        let mut found: Vec<InstanceId> = self
            .world
            .query_pipeline()
            .intersect_aabb_conservative(aabb)
            .filter_map(|(h, co)| {
                let other = *self.colliders.get(&h)?;
                match self.terrain.material(h) {
                    // Solid terrain counts whatever the part's CanCollide,
                    // and so does a part inside it (observed); water doesn't.
                    Some(_) => (!self.terrain.is_water(h)
                        && (overlaps(&*shape, &pose, co) || self.inside_terrain(center)))
                    .then_some(other),
                    None => (others.is_some_and(|all| other != part && (all || collides(other)))
                        && overlaps(&*shape, &pose, co))
                    .then_some(other),
                }
            })
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }
}

/// Whether `shape` at `pose` overlaps a collider by more than [`OVERLAP`].
fn overlaps(shape: &dyn Shape, pose: &Pose, co: &Collider) -> bool {
    let deep = |other: &dyn Shape| {
        parry::contact(pose, shape, co.position(), other, 0.0)
            .ok()
            .flatten()
            .is_some_and(|c| c.dist < -OVERLAP)
    };
    match co.shape().as_trimesh() {
        // Terrain: parry's contact query takes its triangles one at a time.
        Some(mesh) => {
            let local = shape.compute_aabb(&(co.position().inverse() * *pose));
            mesh.bvh()
                .intersect_aabb(&local)
                .any(|i| deep(&mesh.triangle(i)))
        }
        None => deep(co.shape()),
    }
}

/// A unit normal with float noise around zero removed, so a face's
/// normal reads (0, 0.71, -0.71) as in Roblox, not (-0, 0.71, -0.71).
fn surface_normal(n: Vec3) -> Vec3 {
    let n = n.normalize_or_zero();
    Vec3::select(n.abs().cmplt(Vec3::splat(1e-4)), Vec3::ZERO, n)
}
