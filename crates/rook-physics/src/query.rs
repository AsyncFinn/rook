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
mod filter;

use filter::Accepting;
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
        let ray = Ray::new(origin, dir);
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
        drop(accept);
        // Terrain is a surface, hit only from outside, and only nearer
        // than the part hit: its chunks are built only that far.
        let reach = parts.map_or(max, |(.., hit)| hit.time_of_impact);
        let terrain = if self.accepts_terrain(dom, filter) {
            self.first_terrain_along((origin, dir, reach), |physics, upto| {
                physics.terrain_hit(dom, filter, &ray, upto)
            })
        } else {
            None
        };
        let (h, part, hit) = terrain.or(parts)?;
        let distance = hit.time_of_impact;
        Some(Hit {
            part,
            position: origin + dir * distance,
            normal: surface_normal(hit.normal),
            distance,
            material: self.material_of(dom, h, part),
        })
    }

    /// Whether `filter` lets a query see the Terrain at all.
    fn accepts_terrain(&self, dom: &Dom, filter: &Filter) -> bool {
        (self.terrain.instance)
            .is_some_and(|t| self.accepting(dom, filter).is_some_and(|a| a.accepts(t)))
    }

    /// The nearest built terrain surface `ray` hits from outside within
    /// `max` studs that `filter` accepts.
    fn terrain_hit(
        &self,
        dom: &Dom,
        filter: &Filter,
        ray: &Ray,
        max: f32,
    ) -> Option<(ColliderHandle, InstanceId, RayIntersection)> {
        let accept = self.part_filter(dom, filter);
        let is_terrain = |h: ColliderHandle, _: &Collider| self.terrain.material(h).is_some();
        self.world
            .query_pipeline_with_filter(QueryFilter::new().predicate(&is_terrain))
            .intersect_ray(*ray, max, true)
            .filter_map(|(h, ..)| {
                let hit = self.terrain_ray(h, ray, max)?;
                Some((h, accept(h)?, hit))
            })
            .filter(|(.., hit)| hit.time_of_impact > 0.0)
            .min_by(|a, b| a.2.time_of_impact.total_cmp(&b.2.time_of_impact))
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
        let to = cast.compute_aabb(&(Pose::from_translation(direction) * start));
        self.build_terrain_in(from.mins.min(to.mins), from.maxs.max(to.maxs));
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
        let (h, hit) = pipeline.cast_shape(&start, dir, &*cast, options)?;
        let part = *self.colliders.get(&h)?;
        // witness1 and normal1 are on the part hit, in world space.
        Some(Hit {
            part,
            position: hit.witness1,
            normal: surface_normal(hit.normal1),
            distance: hit.time_of_impact,
            material: self.material_of(dom, h, part),
        })
    }

    /// `Model:MoveTo`'s rule: how far up parts going to these places must
    /// go to overlap no part they collide with. Each overlap lifts them to
    /// where they'd rest on that part if dropped from above; the check
    /// repeats there, so a gap too small to fit in is passed. Only colliding
    /// parts count, on both sides, and the moved parts' own group's rules;
    /// Terrain never obstructs (all observed).
    pub fn rise_out_of_parts(&mut self, dom: &mut Dom, moved: &[(InstanceId, CFrame)]) -> f32 {
        profiling::scope!("Physics::rise_out_of_parts");
        self.prepare(dom);
        let own: FxHashSet<InstanceId> = moved.iter().map(|(p, _)| *p).collect();
        let mut shapes = Vec::with_capacity(moved.len());
        for &(part, to) in moved {
            if dom.bool_property(part, "CanCollide") == Some(false) {
                continue;
            }
            let Some((shape, local)) = shape::local_shape(dom, &mut self.saved, part) else {
                continue;
            };
            let (groups, _) = self.interaction_groups(dom, part);
            shapes.push((shape, pose_of(&to) * local, groups));
        }
        let pipeline = self.world.query_pipeline();
        let mut rise = 0.0_f32;
        loop {
            let mut next = rise;
            for (shape, pose, groups) in &shapes {
                let at = Pose::from_translation(Vec3::Y * rise) * *pose;
                let clearance = if rise > 0.0 { RISE_CLEARANCE } else { 0.0 };
                let tested = Pose::from_translation(Vec3::Y * clearance) * at;
                let aabb = shape.compute_aabb(&tested);
                for (h, co) in pipeline.intersect_aabb_conservative(aabb) {
                    let blocks = !co.is_sensor()
                        && self.terrain.material(h).is_none()
                        && co.solver_groups().test(*groups)
                        && self.colliders.get(&h).is_some_and(|p| !own.contains(p));
                    if blocks && sinks_into(&tested, shape, co) {
                        next = next.max(rise + rest_above(shape, &at, co));
                    }
                }
            }
            if next <= rise {
                return rise;
            }
            rise = next;
        }
    }

    /// The `Enum.Material` value a hit on collider `h` of `part` reports:
    /// the part's, or the terrain material the collider is made of.
    pub(crate) fn material_of(&self, dom: &Dom, h: ColliderHandle, part: InstanceId) -> u32 {
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
            .collect::<FxHashSet<_>>()
            .into_iter()
            .collect();
        found.sort_unstable();
        if max > 0 {
            found.truncate(max);
        }
        found
    }

    /// Parts whose `Size` box meets `shape` at `pose`: Region3 (an
    /// axis-aligned box), `GetPartBoundsInBox` and `GetPartBoundsInRadius`,
    /// in id order, at most `max` of them (0: all). Candidates come from
    /// the parts' boxes ([`bounds`]), so a turned cylinder or wedge is
    /// found near its box's corners too.
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
        let aabb = shape.compute_aabb(&pose);
        // Built from the fields it reads: the boxes below are borrowed too.
        let Some(props) = self.props.as_ref() else {
            return Vec::new();
        };
        let accepting = Accepting::new(filter, dom, props, &self.groups);
        let mut found: Vec<InstanceId> = self
            .bounds
            .near(dom, &aabb)
            .filter(|&part| {
                accepting.accepts(part)
                    && shape::world_box(dom, part).is_some_and(|(b, at)| {
                        parry::intersection_test(&pose, &*shape, &at, &*b)
                            .is_ok_and(|t| t.intersecting)
                    })
            })
            .collect();
        found.sort_unstable();
        if max > 0 {
            found.truncate(max);
        }
        found
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
        let Some((shape, pose)) = shape::world_shape(dom, &mut self.saved, part) else {
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
        let Some((shape, pose)) = shape::world_shape(dom, &mut self.saved, part) else {
            return Vec::new();
        };
        let aabb = shape.compute_aabb(&pose);
        let center = pose.translation;
        self.build_terrain_in(aabb.mins, aabb.maxs);
        // Solid terrain counts whatever the part's CanCollide, and so does
        // a part inside it (observed); water doesn't.
        let mut buried_in = None;
        let mut found: Vec<InstanceId> = Vec::new();
        for (h, co) in self
            .world
            .query_pipeline()
            .intersect_aabb_conservative(aabb)
        {
            let Some(&other) = self.colliders.get(&h) else {
                continue;
            };
            let touching = match self.terrain.material(h) {
                Some(_) if self.terrain.is_water(h) => false,
                Some(_) => {
                    let touching = overlaps(&*shape, &pose, co);
                    if !touching {
                        buried_in = Some(other);
                    }
                    touching
                }
                None => {
                    others.is_some_and(|all| other != part && (all || collides(other)))
                        && overlaps(&*shape, &pose, co)
                }
            };
            if touching {
                found.push(other);
            }
        }
        // Only a piece of terrain around it that it doesn't touch asks
        // whether it's inside, which looks up from it.
        if let Some(terrain) = buried_in
            && self.inside_terrain(center)
        {
            found.push(terrain);
        }
        found.sort_unstable();
        found.dedup();
        found
    }
}

/// Whether `shape` at `pose` is sunk into `co`, not just touching it. A hull
/// well inside a shape can report no depth at all, so a depth of about nothing
/// is settled by nudging it each way: what's touching comes apart in one.
fn sinks_into(pose: &Pose, shape: &SharedShape, co: &Collider) -> bool {
    match parry::contact(pose, &**shape, co.position(), co.shape(), 0.0) {
        Ok(Some(c)) if c.dist.abs() > RISE_OVERLAP => c.dist < 0.0,
        Ok(None) => false,
        _ => [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ]
        .iter()
        .all(|d| {
            let nudged = Pose::from_translation(*d * RISE_OVERLAP) * *pose;
            parry::intersection_test(&nudged, &**shape, co.position(), co.shape())
                .is_ok_and(|t| t.intersecting)
        }),
    }
}

/// How far up from `pose` `shape` comes to rest on `co`, dropped from above.
fn rest_above(shape: &SharedShape, pose: &Pose, co: &Collider) -> f32 {
    let start_above = co.compute_aabb().maxs.y - shape.compute_aabb(pose).mins.y + 1.0;
    let start = Pose::from_translation(Vec3::Y * start_above) * *pose;
    let options = ShapeCastOptions {
        max_time_of_impact: f32::MAX,
        target_distance: 0.0,
        stop_at_penetration: false,
        compute_impact_geometry_on_penetration: false,
    };
    parry::cast_shapes(
        &start,
        Vec3::NEG_Y,
        &**shape,
        co.position(),
        Vec3::ZERO,
        co.shape(),
        options,
    )
    .ok()
    .flatten()
    .map_or(start_above - 1.0, |hit| start_above - hit.time_of_impact)
}

/// How much a shape must sink into another for `Model:MoveTo` to lift it:
/// only float noise at the coordinates Studio measured it at (1,000 studs up;
/// 0.001 lifts a model, a face merely meeting another doesn't).
const RISE_OVERLAP: f32 = 5e-5;

/// How close to something above a rising shape may stop: a gap this much
/// taller than it is passed (observed: 0.01 passed, 0.02 stops it).
const RISE_CLEARANCE: f32 = 0.015;

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
