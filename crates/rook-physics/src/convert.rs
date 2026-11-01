//! Between Rook's CFrames (a rotation matrix) and Rapier's poses (a
//! quaternion). Both use the same glam, so vectors need no conversion.

use rapier3d::math::Pose;
use rook_types::CFrame;

pub(crate) fn pose_of(cf: &CFrame) -> Pose {
    Pose::from_parts(cf.position, glam::Quat::from_mat3(&cf.rotation).normalize())
}

/// The pose `along` the way from `a` to `b` (0 to 1).
pub(crate) fn between(a: &Pose, b: &Pose, along: f32) -> CFrame {
    CFrame {
        position: a.translation.lerp(b.translation, along),
        rotation: glam::Mat3::from_quat(a.rotation.slerp(b.rotation, along)),
    }
}

pub(crate) fn cframe_of(pose: &Pose) -> CFrame {
    CFrame {
        position: pose.translation,
        rotation: glam::Mat3::from_quat(pose.rotation),
    }
}
