//! Lifting moved parts out of the parts they would sink into.

use super::*;

impl Physics {
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
}

/// Whether `shape` at `pose` is sunk into `co`, not just touching it. A hull
/// well inside a shape can report no depth at all, so a depth of about nothing
/// is settled by nudging it each way: what's touching comes apart in one.
pub(super) fn sinks_into(pose: &Pose, shape: &SharedShape, co: &Collider) -> bool {
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
pub(super) fn rest_above(shape: &SharedShape, pose: &Pose, co: &Collider) -> f32 {
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
pub(super) const RISE_OVERLAP: f32 = 5e-5;

/// How close to something above a rising shape may stop: a gap this much
/// taller than it is passed (observed: 0.01 passed, 0.02 stops it).
pub(super) const RISE_CLEARANCE: f32 = 0.015;
