//! Rays and swept shapes cast through the parts and terrain, stopping at
//! the first hit the filter accepts.

use super::*;

impl Physics {
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

    /// Every part a ray from `origin` passes before its end at `origin +
    /// direction`, nearest first, however long it is: a part the ray
    /// starts in or only reaches at its end is left out, one holding its
    /// end is in, and Terrain never is (all observed, for
    /// `Camera:GetPartsObscuringTarget`).
    pub fn parts_along(
        &mut self,
        dom: &mut Dom,
        origin: Vec3,
        direction: Vec3,
        filter: &Filter,
    ) -> Vec<InstanceId> {
        profiling::scope!("Physics::parts_along");
        self.prepare(dom);
        let dom = &*dom;
        let length = direction.length();
        if !length.is_finite() || length <= 0.0 || !origin.is_finite() {
            return Vec::new();
        }
        let ray = Ray::new(origin, direction / length);
        let accept = self.part_filter(dom, filter);
        let is_part = |h: ColliderHandle, _: &Collider| self.terrain.material(h).is_none();
        let mut hits: Vec<(f32, InstanceId)> = self
            .world
            .query_pipeline_with_filter(QueryFilter::new().predicate(&is_part))
            .intersect_ray(ray, length, true)
            .filter(|(.., hit)| hit.time_of_impact > 0.0 && hit.time_of_impact < length)
            .filter_map(|(h, _, hit)| Some((hit.time_of_impact, accept(h)?)))
            .collect();
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        // A part of several colliders comes once, where the ray meets it.
        let mut seen = FxHashSet::default();
        hits.into_iter()
            .filter_map(|(_, part)| seen.insert(part).then_some(part))
            .collect()
    }

    /// Whether `filter` lets a query see the Terrain at all.
    pub(super) fn accepts_terrain(&self, dom: &Dom, filter: &Filter) -> bool {
        (self.terrain.instance)
            .is_some_and(|t| self.accepting(dom, filter).is_some_and(|a| a.accepts(t)))
    }

    /// The nearest built terrain surface `ray` hits from outside within
    /// `max` studs that `filter` accepts.
    pub(super) fn terrain_hit(
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
        let cast = shape.shape();
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
}

/// A unit normal with float noise around zero removed, so a face's
/// normal reads (0, 0.71, -0.71) as in Roblox, not (-0, 0.71, -0.71).
pub(super) fn surface_normal(n: Vec3) -> Vec3 {
    let n = n.normalize_or_zero();
    Vec3::select(n.abs().cmplt(Vec3::splat(1e-4)), Vec3::ZERO, n)
}
