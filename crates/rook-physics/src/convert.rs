//! Between Rook's math types (glam) and Rapier's (glamx, a different glam
//! version).

use rapier3d::math::{Pose, Rotation, Vector};
use rook_types::CFrame;

pub(crate) fn to_rapier(v: glam::Vec3) -> Vector {
    Vector::new(v.x, v.y, v.z)
}

pub(crate) fn to_glam(v: Vector) -> glam::Vec3 {
    glam::Vec3::new(v.x, v.y, v.z)
}

pub(crate) fn pose_of(cf: &CFrame) -> Pose {
    let q = glam::Quat::from_mat3(&cf.rotation).normalize();
    Pose::from_parts(
        to_rapier(cf.position),
        Rotation::from_xyzw(q.x, q.y, q.z, q.w),
    )
}

/// The pose `along` the way from `a` to `b` (0 to 1).
pub(crate) fn between(a: &Pose, b: &Pose, along: f32) -> CFrame {
    let (a, b) = (cframe_of(a), cframe_of(b));
    let turn = glam::Quat::from_mat3(&a.rotation).slerp(glam::Quat::from_mat3(&b.rotation), along);
    CFrame {
        position: a.position.lerp(b.position, along),
        rotation: glam::Mat3::from_quat(turn),
    }
}

pub(crate) fn cframe_of(pose: &Pose) -> CFrame {
    let r = pose.rotation;
    let q = glam::Quat::from_xyzw(r.x, r.y, r.z, r.w);
    CFrame {
        position: to_glam(pose.translation),
        rotation: glam::Mat3::from_quat(q),
    }
}
