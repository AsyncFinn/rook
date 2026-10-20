//! FloorWire: a wire laid along the floor between two parts, drawn with
//! its `From` part.

use glam::Quat;
use rook_types::PartShape;

use super::*;

/// How much bigger than the wire its corner balls are (measured: about
/// 1.25 × `WireRadius`, so they bulge at the corners).
const CORNER: f32 = 1.25;

/// One FloorWire from a part at `from`: tubes `WireRadius` thick (0.0625
/// by default) in `Color3`, lit, with a ball at each corner. Measured in
/// Studio 0.742 (2026-10-20, parts 20 studs apart on every axis, one
/// over a platform): the wire drops from `From`'s centre to the floor
/// under it (the top of what's below, the wire's centre on it), runs
/// along X, then along Z, at that height, and rises to `To`'s centre.
/// Rook finds the floor from the parts' world boxes (Approximation: a
/// turned part's box reaches past it). Rook draws nothing without a `To`.
/// Not yet: its moving `Texture`, and following `To` when only `To` moves.
pub(super) fn floor_wire(dom: &Dom, id: InstanceId, from: CFrame) -> Vec<(PartInstance, Pose)> {
    let Some(to_part) = dom.ref_property(id, "To") else {
        return Vec::new();
    };
    let Some(to) = dom.cframe_property(to_part, "CFrame") else {
        return Vec::new();
    };
    let radius = dom.float_property(id, "WireRadius").unwrap_or(0.0625);
    let wire = PartInstance {
        light: Light::Lit,
        ..piece(dom, id, DrawShape::Adorn(AdornShape::Tube { hole: 0 }))
    };
    if radius <= 0.0 || wire.transparency >= 1.0 {
        return Vec::new();
    }
    let (a, b) = (from.position, to.position);
    let skip = [dom.ref_property(id, "From"), Some(to_part)];
    let floor = floor_under(dom, a, &skip).unwrap_or(a.y);
    let path = [
        a,
        Vec3::new(a.x, floor, a.z),
        Vec3::new(b.x, floor, a.z),
        Vec3::new(b.x, floor, b.z),
        b,
    ];
    // Into the From part's axes.
    let local = |p: Vec3| from.rotation.transpose() * (p - from.position);
    let mut out = Vec::new();
    for (i, leg) in path.windows(2).enumerate() {
        let (p, q) = (local(leg[0]), local(leg[1]));
        let length = p.distance(q);
        if length < 1e-4 {
            continue;
        }
        let turn = Mat3::from_quat(Quat::from_rotation_arc(Vec3::Z, (q - p) / length));
        let tube = PartInstance {
            size: Vec3::new(2.0 * radius, 2.0 * radius, length),
            ..wire
        };
        out.push((
            tube,
            Pose {
                offset: (p + q) * 0.5,
                turn,
            },
        ));
        if i > 0 {
            let ball = PartInstance {
                shape: DrawShape::Part(PartShape::Ball),
                size: Vec3::splat(2.0 * radius * CORNER),
                ..wire
            };
            out.push((ball, at(p)));
        }
    }
    out
}

/// The top of the highest Workspace part under `at` (not those in `skip`).
fn floor_under(dom: &Dom, at: Vec3, skip: &[Option<InstanceId>]) -> Option<f32> {
    let mut floor: Option<f32> = None;
    for part in workspace_parts(dom) {
        if skip.contains(&Some(part)) {
            continue;
        }
        let (Some(size), Some(cf)) = (
            dom.vector3_property(part, "Size"),
            dom.cframe_property(part, "CFrame"),
        ) else {
            continue;
        };
        let r = cf.rotation;
        let half = Mat3::from_cols(r.x_axis.abs(), r.y_axis.abs(), r.z_axis.abs()) * (size * 0.5);
        let (lo, hi) = (cf.position - half, cf.position + half);
        let over = at.x >= lo.x && at.x <= hi.x && at.z >= lo.z && at.z <= hi.z;
        if over && hi.y <= at.y && floor.is_none_or(|f| hi.y > f) {
            floor = Some(hi.y);
        }
    }
    floor
}
