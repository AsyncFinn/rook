//! Handles and ArcHandles: the grips tools and plugins drag, around their
//! Adornee. Dragging them comes with GUI input (M8).
//!
//! Measured in Studio 0.742 (2026-10-20, 1-, 4- and 8-stud cubes, 60 to 120
//! studs away at fields of view of 20° and 40°): the grips keep one size on
//! screen whatever the field of view, growing with distance; where they
//! sit stays in studs. Sizes here are as drawn 100 studs away.

use glam::Quat;
use rook_types::{Axes, Faces, PartShape};

use super::*;

/// Grips are drawn at their size 100 studs away times this per stud.
const PER_STUD: f32 = 0.01;
/// A grip ball's diameter (3.6 at 100 studs, 2.2 at 60).
const BALL: f32 = 3.6;

/// Handles: on each of `Faces`, a ball 2 studs out from the face (Style
/// Resize, the default) or an arrow pointing out of it (Movement): a
/// shaft from just off the face and a cone, reaching 19 studs at 100
/// studs away. In `Color3`, lit, a little see-through (Inference: about
/// 0.2, from the part showing through them).
pub(super) fn handles(dom: &Dom, id: InstanceId, size: Vec3) -> Vec<(PartInstance, Pose)> {
    let faces = dom.faces_property(id, "Faces").map_or(0x3f, Faces::bits);
    let movement = dom.enum_item_property(id, "Style") == Some("Movement");
    let base = PartInstance {
        light: Light::Lit,
        transparency: 1.0 - (1.0 - dom.float_property(id, "Transparency").unwrap_or(0.0)) * 0.8,
        ..piece(dom, id, DrawShape::Part(PartShape::Ball))
    };
    if base.transparency >= 1.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for face in NormalId::ALL {
        if faces & (1 << face.index()) == 0 {
            continue;
        }
        let normal = face.normal();
        let on_face = normal * (size.abs() * 0.5).dot(normal.abs());
        if !movement {
            out.push(grip(base, on_face + normal * 2.0, Vec3::splat(BALL)));
            continue;
        }
        let (gap, reach, cone, cone_width, shaft) = (0.67, 19.0, 4.0, 2.1, 0.57);
        let along = Mat3::from_quat(Quat::from_rotation_arc(Vec3::Z, normal));
        let shaft_length = reach - cone - gap;
        let mut pieces = [
            (
                AdornShape::Tube { hole: 0 },
                gap + shaft_length * 0.5,
                Vec3::new(shaft, shaft, shaft_length),
                along,
            ),
            (
                AdornShape::Cone,
                reach - cone * 0.5,
                Vec3::new(cone_width, cone_width, cone),
                // The cone's tip is at its -Z.
                Mat3::from_quat(Quat::from_rotation_arc(-Vec3::Z, normal)),
            ),
        ];
        for (shape, out_by, extent, turn) in &mut pieces {
            let part = PartInstance {
                shape: DrawShape::Adorn(*shape),
                size: *extent,
                grows: Some(Grow {
                    anchor: on_face,
                    per_stud: PER_STUD,
                }),
                ..base
            };
            let pose = Pose {
                offset: on_face + normal * *out_by,
                turn: *turn,
            };
            out.push((part, pose));
        }
    }
    out
}

/// ArcHandles: for each of `Axes`, a thin ring around it (X red, Y green,
/// Z blue, whatever `Color3`) of the part's bounding-sphere radius plus 2
/// studs (2.9, 5.5 and 8.9 on 1-, 4- and 8-stud cubes), with a ball on it
/// at either end of the next axis: the X ring's at ±Z, Y's at ±X and Z's
/// at ±Y. The rings stay about two pixels thick (Rook: 0.1 studs).
pub(super) fn arc_handles(dom: &Dom, id: InstanceId, size: Vec3) -> Vec<(PartInstance, Pose)> {
    let axes = dom.axes_property(id, "Axes").map_or(0b111, Axes::bits);
    let base = PartInstance {
        light: Light::Lit,
        ..piece(dom, id, DrawShape::Part(PartShape::Ball))
    };
    if base.transparency >= 1.0 {
        return Vec::new();
    }
    let radius = size.length() * 0.5 + 2.0;
    let width = 0.1;
    let mut out = Vec::new();
    for axis in 0..3 {
        if axes & (1 << axis) == 0 {
            continue;
        }
        let mut color = [0.0; 3];
        color[axis] = 1.0;
        let (normal, ball_at) = (Vec3::AXES[axis], Vec3::AXES[(axis + 2) % 3]);
        let ring = PartInstance {
            shape: DrawShape::Adorn(AdornShape::Tube {
                hole: AdornShape::hole(radius - width, radius),
            }),
            size: Vec3::new(2.0 * radius, 2.0 * radius, width),
            color,
            ..base
        };
        let turn = Mat3::from_quat(Quat::from_rotation_arc(Vec3::Z, normal));
        out.push((
            ring,
            Pose {
                offset: Vec3::ZERO,
                turn,
            },
        ));
        for side in [-1.0, 1.0] {
            let ball = PartInstance { color, ..base };
            out.push(grip(ball, ball_at * side * radius, Vec3::splat(BALL)));
        }
    }
    out
}

/// A ball grip `at` (in the part's axes) keeping one size on screen.
fn grip(base: PartInstance, at: Vec3, size: Vec3) -> (PartInstance, Pose) {
    let part = PartInstance {
        shape: DrawShape::Part(PartShape::Ball),
        size,
        grows: Some(Grow {
            anchor: at,
            per_stud: PER_STUD,
        }),
        ..base
    };
    (part, super::at(at))
}
