//! Feeling the floor: rays down from the root part's bottom, at its center
//! and its footprint's corners. The floor's height is the average of the
//! rays that hit (Inference from Studio: the floor holds until the
//! footprint has left a ledge, a step lifts the root as the footprint
//! moves onto it, steps up to the root's bottom are walked up on R15 and
//! R6 alike and taller ones aren't, and on a slope the root follows the
//! height under its center).

use super::*;

/// How far below standing height the floor is still felt (Inference:
/// FloorMaterial kept the floor's until about 2.5 studs into a jump).
pub(super) const REACH: f32 = 2.5;

/// The floor under a Humanoid.
#[derive(Debug, Clone, Copy)]
pub(super) struct Floor {
    /// Its height under the root's center.
    pub(super) y: f32,
    /// Its averaged surface normal.
    pub(super) normal: glam::Vec3,
    /// How fast it moves where the root stands (a platform's, or a
    /// conveyor's surface velocity).
    pub(super) velocity: glam::Vec3,
    pub(super) part: InstanceId,
    pub(super) collider: ColliderHandle,
}

impl Physics {
    pub(super) fn sense_floor(
        &self,
        c: &Controller,
        body: RigidBodyHandle,
        root: &CFrame,
    ) -> Option<Floor> {
        let drive = &c.drive;
        let half = drive.root_size * 0.5;
        let right = root.rotation * glam::Vec3::X;
        let back = root.rotation * glam::Vec3::Z;
        let right = horizontal(right).normalize_or_zero() * half.x;
        let back = horizontal(back).normalize_or_zero() * half.z;
        let starts = [
            glam::Vec3::ZERO,
            right + back,
            right - back,
            -right + back,
            -right - back,
        ];
        // Getting up, the root stands up where it lay, its bottom at or
        // under the floor, and has to feel that floor to rise off it.
        let start = if c.state == State::GettingUp {
            0.0
        } else {
            half.y
        };
        let bottom = root.position - glam::Vec3::Y * start;
        let length = drive.standing_height - start + REACH;
        let filter = self.humanoid_filter(body);
        let pipeline = self.world.query_pipeline_with_filter(filter);
        let mut hits = 0.0;
        let mut height = 0.0;
        let mut normal = glam::Vec3::ZERO;
        let mut first = None;
        for offset in starts {
            let origin = bottom + offset;
            let ray = Ray::new(origin, Vector::NEG_Y);
            // A ray starting inside a part (a step too tall) feels nothing.
            let Some((h, hit)) = pipeline
                .cast_ray_and_get_normal(&ray, length, true)
                .filter(|(_, hit)| hit.time_of_impact > 0.0)
            else {
                continue;
            };
            hits += 1.0;
            height += origin.y - hit.time_of_impact;
            normal += hit.normal;
            if first.is_none() {
                first = Some((h, origin - glam::Vec3::Y * hit.time_of_impact));
            }
        }
        let (collider, point) = first?;
        let part = *self.colliders.get(&collider)?;
        Some(Floor {
            y: height / hits,
            normal: normal.normalize_or(glam::Vec3::Y),
            velocity: self.velocity_at(collider, point),
            part,
            collider,
        })
    }

    /// Queries a Humanoid makes: past its own body and parts with
    /// CanCollide off (sensors), against what its root collides with.
    pub(super) fn humanoid_filter(&self, body: RigidBodyHandle) -> QueryFilter<'_> {
        let groups = self.bodies.get(&body).and_then(|b| {
            let (collider, _) = b.colliders.iter().flatten().next()?;
            Some(self.world.colliders.get(*collider)?.collision_groups())
        });
        let filter = QueryFilter::new()
            .exclude_rigid_body(body)
            .exclude_sensors();
        match groups {
            Some(g) => filter.groups(g),
            None => filter,
        }
    }

    /// How fast the surface of `collider` moves at `point`: a moving body's
    /// velocity there, or an anchored part's conveyor velocity.
    fn velocity_at(&self, collider: ColliderHandle, point: glam::Vec3) -> glam::Vec3 {
        let Some(rb) = (self.world.colliders.get(collider))
            .and_then(Collider::parent)
            .and_then(|b| self.world.bodies.get(b))
        else {
            return glam::Vec3::ZERO;
        };
        if rb.is_fixed() {
            contacts::unpack_velocity(rb.user_data)
        } else {
            rb.velocity_at_point(point)
        }
    }
}
