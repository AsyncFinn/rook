//! Derived properties: values computed from other properties rather than
//! stored. `get` and `set` return `None` for properties stored normally.

use glam::{EulerRot, Mat3};
use raven_dom::{DomError, InstanceId};
use raven_types::{CFrame, Color3, Value, Vector3};

use crate::types::color::closest_brick_color;
use crate::world::World;

fn cframe(w: &World, id: InstanceId) -> CFrame {
    match w.dom.property_by_name(id, "CFrame") {
        Some(Value::CFrame(c)) => c,
        _ => CFrame::IDENTITY,
    }
}

fn set_cframe(w: &mut World, id: InstanceId, c: CFrame) -> Result<(), DomError> {
    let class = w.dom.get(id).ok_or(DomError::Stale)?.class;
    let pid = w
        .dom
        .database()
        .find_property(class, "CFrame")
        .expect("BasePart.CFrame")
        .id;
    w.dom.set_property(id, pid, Value::CFrame(c))
}

fn degrees(v: (f32, f32, f32)) -> Vector3 {
    Vector3::new(v.0.to_degrees(), v.1.to_degrees(), v.2.to_degrees())
}

/// `Orientation`: degrees, applied in Y, X, Z order (Confirmed: creator-docs
/// BasePart.Orientation).
fn orientation(c: &CFrame) -> Vector3 {
    let (y, x, z) = c.rotation.to_euler(EulerRot::YXZ);
    degrees((x, y, z))
}

/// `Rotation`: degrees, the XYZ Euler angles (Confirmed: creator-docs
/// BasePart.Rotation).
fn rotation(c: &CFrame) -> Vector3 {
    degrees(c.to_euler_angles_xyz())
}

pub(crate) fn get(w: &World, id: InstanceId, name: &str) -> Option<Value> {
    let dom = &w.dom;
    match name {
        "Position" | "Orientation" | "Rotation" if dom.is_a(id, "BasePart") => {
            let c = cframe(w, id);
            Some(Value::Vector3(match name {
                "Position" => Vector3(c.position),
                "Orientation" => orientation(&c),
                _ => rotation(&c),
            }))
        }
        "BrickColor" | "brickColor" if dom.is_a(id, "BasePart") => {
            match dom.property_by_name(id, "Color") {
                Some(Value::Color3(c)) => Some(Value::BrickColor(closest_brick_color(c))),
                _ => None,
            }
        }
        "Enabled" if dom.is_a(id, "BaseScript") => {
            let disabled = matches!(
                dom.property_by_name(id, "Disabled"),
                Some(Value::Bool(true))
            );
            Some(Value::Bool(!disabled))
        }
        "WorldPivot" if dom.is_a(id, "Model") => {
            if let Some(Value::OptionalCFrame(Some(c))) = dom.property_by_name(id, "WorldPivotData")
            {
                return Some(Value::CFrame(c));
            }
            // Without stored pivot data, the pivot is the PrimaryPart's CFrame
            // (Confirmed: creator-docs PVInstance pivots). The bounding-box
            // fallback needs part geometry and arrives with physics.
            match dom.property_by_name(id, "PrimaryPart") {
                Some(Value::Ref(Some(r))) => {
                    Some(Value::CFrame(cframe(w, InstanceId::from_ref(r))))
                }
                _ => Some(Value::CFrame(CFrame::IDENTITY)),
            }
        }
        _ => None,
    }
}

pub(crate) fn set(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<Result<(), DomError>> {
    match (name, value) {
        ("Position", Value::Vector3(p)) if w.dom.is_a(id, "BasePart") => {
            let mut c = cframe(w, id);
            c.position = p.0;
            Some(set_cframe(w, id, c))
        }
        ("Orientation", Value::Vector3(v)) if w.dom.is_a(id, "BasePart") => {
            let mut c = cframe(w, id);
            let r = |d: f32| d.to_radians();
            c.rotation = Mat3::from_euler(EulerRot::YXZ, r(v.y()), r(v.x()), r(v.z()));
            Some(set_cframe(w, id, c))
        }
        ("Rotation", Value::Vector3(v)) if w.dom.is_a(id, "BasePart") => {
            let mut c = cframe(w, id);
            let r = |d: f32| d.to_radians();
            c.rotation = CFrame::angles(r(v.x()), r(v.y()), r(v.z())).rotation;
            Some(set_cframe(w, id, c))
        }
        ("BrickColor" | "brickColor", Value::BrickColor(b)) if w.dom.is_a(id, "BasePart") => {
            let class = w.dom.get(id)?.class;
            let pid = w.dom.database().find_property(class, "Color")?.id;
            Some(w.dom.set_property(id, pid, Value::Color3(Color3::from(*b))))
        }
        ("Enabled", Value::Bool(b)) if w.dom.is_a(id, "BaseScript") => {
            let class = w.dom.get(id)?.class;
            let pid = w.dom.database().find_property(class, "Disabled")?.id;
            Some(w.dom.set_property(id, pid, Value::Bool(!b)))
        }
        ("WorldPivot", Value::CFrame(c)) if w.dom.is_a(id, "Model") => {
            let class = w.dom.get(id)?.class;
            let pid = w.dom.database().find_property(class, "WorldPivotData")?.id;
            Some(w.dom.set_property(id, pid, Value::OptionalCFrame(Some(*c))))
        }
        _ => None,
    }
}
