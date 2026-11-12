//! A Tool's `GripPos`, `GripForward`, `GripUp` and `GripRight`: views of
//! its `Grip`, whose position and right, up and back columns they read
//! (`GripForward` is the back column negated). Measured in Studio 0.747
//! (docs/behavior/characters/tools.md#the-grip): writing a direction keeps
//! it, makes one neighbor perpendicular to it again and crosses the two
//! for the third, keeping the position.

use glam::{Mat3, Vec3};

use super::*;

/// What a degenerate write leaves: the neighbor kept was parallel to the
/// new direction. Measured: always this matrix, whatever was written
/// (a left-handed one, kept as Studio has it; the classic sword's lunge
/// passes through it).
const DEGENERATE: Mat3 = Mat3::from_cols(
    Vec3::new(0.0, -1.0, 0.0),
    Vec3::new(1.0, 0.0, 0.0),
    Vec3::new(0.0, 0.0, -1.0),
);

fn grip(w: &World, id: InstanceId) -> CFrame {
    w.dom
        .cframe_property(id, "Grip")
        .unwrap_or(CFrame::IDENTITY)
}

pub(super) fn get(w: &World, id: InstanceId, name: &str) -> Option<Value> {
    let g = grip(w, id);
    let v = match name {
        "GripPos" => g.position,
        "GripForward" => -g.rotation.z_axis,
        "GripUp" => g.rotation.y_axis,
        "GripRight" => g.rotation.x_axis,
        _ => return None,
    };
    Some(Value::Vector3(Vector3(v)))
}

/// `v` without its part along unit `axis`, as a unit vector; `None` when
/// nothing is left.
fn perpendicular(v: Vec3, axis: Vec3) -> Option<Vec3> {
    (v - v.dot(axis) * axis).try_normalize()
}

pub(super) fn set(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<Result<(), DomError>> {
    let Value::Vector3(Vector3(v)) = value else {
        return None;
    };
    let g = grip(w, id);
    let r = g.rotation;
    let rotation = match name {
        "GripPos" => {
            let c = CFrame { position: *v, ..g };
            return Some(set_named(w, id, "Grip", Value::CFrame(c)));
        }
        "GripForward" => v.try_normalize().map(|f| -f).and_then(|back| {
            let up = perpendicular(r.y_axis, back)?;
            Some(Mat3::from_cols(up.cross(back), up, back))
        }),
        "GripUp" => v.try_normalize().and_then(|up| {
            let right = perpendicular(r.x_axis, up)?;
            Some(Mat3::from_cols(right, up, right.cross(up)))
        }),
        "GripRight" => v.try_normalize().and_then(|right| {
            let up = perpendicular(r.y_axis, right)?;
            Some(Mat3::from_cols(right, up, right.cross(up)))
        }),
        _ => return None,
    };
    let c = CFrame {
        rotation: rotation.unwrap_or(DEGENERATE),
        ..g
    };
    Some(set_named(w, id, "Grip", Value::CFrame(c)))
}
