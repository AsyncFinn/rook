//! SelectionPartLasso: a bar from a Humanoid's root part
//! (`rook_engine::humanoid_root_part`) to `Part`, drawn with the root
//! part.

use glam::Quat;

use super::*;

/// How thick the bar is, in studs (measured: 9 pixels 40 studs away and 3
/// at 120, at 36 and 12 pixels a stud).
const THICKNESS: f32 = 0.25;

/// One lasso from a root part at `from`. Deprecated, but modern Studio
/// still draws it (observed in 0.742, 2026-10-20): a bar [`THICKNESS`]
/// across from the root part's center to `Part`'s, in `Color3` without
/// light, so the parts hide its ends. It is drawn as a round bar
/// (Inference: only seen side on).
pub(super) fn lasso(dom: &Dom, id: InstanceId, from: CFrame) -> Vec<(PartInstance, Pose)> {
    let Some(to) = dom
        .ref_property(id, "Part")
        .and_then(|p| dom.cframe_property(p, "CFrame"))
    else {
        return Vec::new();
    };
    // In the root part's axes.
    let end = from.rotation.transpose() * (to.position - from.position);
    let length = end.length();
    let bar = piece(dom, id, DrawShape::Adorn(AdornShape::Tube { hole: 0 }));
    if length <= 0.0 || bar.transparency >= 1.0 {
        return Vec::new();
    }
    let bar = PartInstance {
        size: Vec3::new(THICKNESS, THICKNESS, length),
        ..bar
    };
    let pose = Pose {
        offset: end * 0.5,
        turn: Mat3::from_quat(Quat::from_rotation_arc(Vec3::Z, end / length)),
    };
    vec![(bar, pose)]
}
