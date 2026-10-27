//! Climbing: a TrussPart, or a ladder of parts with gaps between them,
//! just ahead of a walking Humanoid. Rays go forward at heights from its
//! feet to above its root (Approximation: Roblox's sensor isn't
//! documented; a truss was taken from about 1.9 studs, root center to
//! face, in Studio).

use super::*;

/// How far ahead of the root's center a truss is taken.
const REACH: f32 = 2.0;

/// How many rays, from half a stud above the feet to `TOP` above the root.
const RAYS: usize = 6;
const TOP: f32 = 1.5;

/// What a Humanoid climbs.
#[derive(Debug, Clone, Copy)]
pub(super) struct Climb {
    /// The face's normal, horizontal, pointing at the Humanoid.
    pub(super) normal: glam::Vec3,
    /// A point on the face.
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
        let step = (root.position.y + TOP - bottom) / (RAYS - 1) as f32;
        let hits: Vec<Option<(ColliderHandle, RayIntersection, glam::Vec3)>> = (0..RAYS)
            .map(|i| {
                let origin =
                    glam::Vec3::new(root.position.x, bottom + step * i as f32, root.position.z);
                let ray = Ray::new(to_rapier(origin), to_rapier(ahead));
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
        let first = hits.iter().position(Option::is_some);
        let last = hits.iter().rposition(Option::is_some);
        let ladder = match (first, last) {
            (Some(a), Some(b)) => hits[a..=b].iter().any(Option::is_none),
            _ => false,
        };
        let (_, hit, point) =
            truss.or_else(|| ladder.then(|| hits.iter().flatten().next()).flatten())?;
        let normal = horizontal(to_glam(hit.normal)).normalize_or_zero();
        (normal != glam::Vec3::ZERO).then_some(Climb {
            normal,
            point: *point,
        })
    }
}
