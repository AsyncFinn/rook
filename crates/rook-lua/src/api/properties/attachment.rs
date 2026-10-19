//! An Attachment's properties that are views of its `CFrame` (creator-docs
//! Attachment): `Position`, `Orientation`, `Axis` and `SecondaryAxis` in
//! its part's space, and the `World` ones in the world's. Setting one sets
//! the `CFrame`, which lights, sounds and effects under it follow.

use super::*;

/// The Attachment's part's CFrame (the world's origin when it isn't under
/// a part).
fn part_frame(w: &World, id: InstanceId) -> CFrame {
    match w.dom.parent(id) {
        Some(p) if w.dom.is_a(p, "BasePart") => cframe(w, p),
        _ => CFrame::IDENTITY,
    }
}

pub(super) fn get(w: &World, id: InstanceId, name: &str) -> Option<Value> {
    let local = cframe(w, id);
    let (c, view) = match name.strip_prefix("World") {
        Some(view) => (part_frame(w, id) * local, view),
        None => (local, name),
    };
    let v = |v: glam::Vec3| Some(Value::Vector3(Vector3(v)));
    match view {
        "Position" => v(c.position),
        "Orientation" => Some(Value::Vector3(orientation(&c))),
        "Axis" => v(c.rotation.x_axis),
        "SecondaryAxis" => v(c.rotation.y_axis),
        "CFrame" if name == "WorldCFrame" => Some(Value::CFrame(c)),
        _ => None,
    }
}

pub(super) fn set(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<Result<(), DomError>> {
    let part = part_frame(w, id);
    let local = cframe(w, id);
    let world = part * local;
    let r = |d: f32| d.to_radians();
    let local = match (name, value) {
        ("Position", Value::Vector3(p)) => CFrame {
            position: p.0,
            ..local
        },
        ("Orientation", Value::Vector3(o)) => CFrame {
            rotation: CFrame::from_orientation(r(o.x()), r(o.y()), r(o.z())).rotation,
            ..local
        },
        ("WorldPosition", Value::Vector3(p)) => {
            part.inverse()
                * CFrame {
                    position: p.0,
                    ..world
                }
        }
        ("WorldOrientation", Value::Vector3(o)) => {
            let rotation = CFrame::from_orientation(r(o.x()), r(o.y()), r(o.z())).rotation;
            part.inverse() * CFrame { rotation, ..world }
        }
        ("WorldCFrame", Value::CFrame(c)) => part.inverse() * *c,
        _ => return None,
    };
    Some(set_named(
        w,
        id,
        "CFrame",
        Value::CFrame(local.orthonormalize()),
    ))
}
