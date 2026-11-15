//! Climbing: a TrussPart, or a ladder of parts with gaps between them,
//! just ahead of a walking Humanoid. Rays go forward at heights from its
//! feet to above its root (Approximation: Roblox's sensor isn't
//! documented; a truss was taken from about 1.9 studs, root center to
//! face, in Studio). A gap is a ray that hits nothing, or hits
//! [`LADDER_DEPTH`] or more behind the nearest hit: rungs on a board.

use super::*;

/// How far ahead of the root's center a truss is taken.
const REACH: f32 = 2.0;

/// Rays go from half a stud above the feet to `TOP` above the root, at
/// most `RAY_SPACING` apart. Measured in Studio on rungs 1.44 apart: 0.2
/// studs tall always climbed, 0.1 and 0.05 caught for a moment and let go.
const TOP: f32 = 1.5;
const RAY_SPACING: f32 = 0.2;

/// How far behind the nearest hit a ray's hit counts as a gap. Measured
/// in Studio with rungs in front of a board: 0.28 studs between their
/// faces never climbed, 0.30 always did, 0.29 sometimes, wherever the
/// climb began (it began 0.9 to 1.15 studs from the rungs either way).
const LADDER_DEPTH: f32 = 0.29;

#[derive(Debug, Clone, Copy)]
pub(super) struct Climb {
    /// The face's normal, horizontal, pointing at the Humanoid.
    pub(super) normal: glam::Vec3,
    pub(super) point: glam::Vec3,
}

impl Physics {
    pub(super) fn sense_climb(
        &self,
        c: &Controller,
        body: RigidBodyHandle,
        root: &CFrame,
    ) -> Option<Climb> {
        if !c.enabled(State::Climbing) {
            return None;
        }
        let ahead = match (c.state, c.climb) {
            (State::Climbing, Some(climb)) => -climb.normal,
            _ => c.drive.walk.normalize_or_zero(),
        };
        if ahead == glam::Vec3::ZERO {
            return None;
        }
        let pipeline = self
            .world
            .query_pipeline_with_filter(self.humanoid_filter(body));
        let bottom = root.position.y - c.drive.standing_height + 0.5;
        let span = (root.position.y + TOP - bottom).max(0.0);
        let rays = (span / RAY_SPACING).ceil() as usize + 1;
        let step = span / (rays - 1).max(1) as f32;
        let hits: Vec<Option<(ColliderHandle, RayIntersection, glam::Vec3)>> = (0..rays)
            .map(|i| {
                let origin =
                    glam::Vec3::new(root.position.x, bottom + step * i as f32, root.position.z);
                let ray = Ray::new(origin, ahead);
                pipeline
                    .cast_ray_and_get_normal(&ray, REACH, true)
                    .map(|(h, hit)| (h, hit, origin + ahead * hit.time_of_impact))
            })
            .collect();
        let truss = hits.iter().flatten().find(|(h, ..)| {
            self.colliders
                .get(h)
                .is_some_and(|p| self.trusses.contains(p))
        });
        // A ladder: rungs hit with a gap between them.
        let nearest = (hits.iter().flatten())
            .map(|(_, hit, _)| hit.time_of_impact)
            .fold(f32::INFINITY, f32::min);
        let rung = |hit: &Option<(_, RayIntersection, _)>| {
            hit.is_some_and(|(_, hit, _)| hit.time_of_impact < nearest + LADDER_DEPTH)
        };
        let first = hits.iter().position(rung);
        let last = hits.iter().rposition(rung);
        let ladder = match (first, last) {
            (Some(a), Some(b)) => !hits[a..=b].iter().all(rung),
            _ => false,
        };
        let (_, hit, point) = truss.or_else(|| {
            ladder
                .then(|| hits.iter().filter(|h| rung(h)).flatten().next())
                .flatten()
        })?;
        let normal = horizontal(hit.normal).normalize_or_zero();
        (normal != glam::Vec3::ZERO).then_some(Climb {
            normal,
            point: *point,
        })
    }
}
