//! Derived properties: values computed from other properties rather than
//! stored. `get` and `set` return `None` for properties stored normally.

use rook_dom::{DomError, InstanceId};
use rook_types::{CFrame, Color3, Value, Vector3};

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
    degrees(c.to_orientation())
}

/// `Rotation`: degrees, the XYZ Euler angles (Confirmed: creator-docs
/// BasePart.Rotation).
fn rotation(c: &CFrame) -> Vector3 {
    degrees(c.to_euler_angles_xyz())
}

/// Unserialized properties that read and write another property of the
/// same object, found by comparing the database's serialization data.
const SAME_AS: &[(&str, &str, &str)] = &[
    ("Humanoid", "Health", "Health_XML"),
    ("Humanoid", "maxHealth", "MaxHealth"),
    ("Player", "userId", "UserId"),
    ("IntConstrainedValue", "Value", "ConstrainedValue"),
    ("DoubleConstrainedValue", "Value", "ConstrainedValue"),
    ("Sound", "Pitch", "PlaybackSpeed"),
    ("GuiObject", "Transparency", "BackgroundTransparency"),
    ("GuiBase2d", "Localize", "AutoLocalize"),
    (
        "BallSocketConstraint",
        "MaxFrictionTorque",
        "MaxFrictionTorqueXml",
    ),
];

/// BrickColor views of Color3 properties: reads give the nearest
/// BrickColor, writes store the BrickColor's colour.
const BRICK_COLOR_OF: &[(&str, &str, &str)] = &[
    ("BasePart", "BrickColor", "Color"),
    ("BasePart", "brickColor", "Color"),
    ("BodyColors", "HeadColor", "HeadColor3"),
    ("BodyColors", "LeftArmColor", "LeftArmColor3"),
    ("BodyColors", "LeftLegColor", "LeftLegColor3"),
    ("BodyColors", "RightArmColor", "RightArmColor3"),
    ("BodyColors", "RightLegColor", "RightLegColor3"),
    ("BodyColors", "TorsoColor", "TorsoColor3"),
    ("GuiObject", "BackgroundColor", "BackgroundColor3"),
    ("GuiObject", "BorderColor", "BorderColor3"),
    ("GuiBase3d", "Color", "Color3"),
];

fn view<'a>(
    w: &World,
    id: InstanceId,
    name: &str,
    table: &'a [(&str, &str, &str)],
) -> Option<&'a str> {
    table
        .iter()
        .find(|(class, prop, _)| *prop == name && w.dom.is_a(id, class))
        .map(|(_, _, target)| *target)
}

fn set_named(w: &mut World, id: InstanceId, name: &str, value: Value) -> Result<(), DomError> {
    w.dom.set_property_by_name(id, name, value)
}

/// `Lighting.TimeOfDay` is "HH:MM:SS"; `ClockTime` is hours as a number.
fn clock_hours(time_of_day: &str) -> f64 {
    let mut parts = time_of_day
        .split(':')
        .map(|p| p.trim().parse::<f64>().unwrap_or(0.0));
    let h = parts.next().unwrap_or(0.0);
    let m = parts.next().unwrap_or(0.0);
    let s = parts.next().unwrap_or(0.0);
    (h + m / 60.0 + s / 3600.0).rem_euclid(24.0)
}

pub(crate) fn time_of_day(hours: f64) -> String {
    let total = (hours.rem_euclid(24.0) * 3600.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600 % 24,
        total / 60 % 60,
        total % 60
    )
}

/// Lighting's clock in hours.
pub(crate) fn lighting_hours(w: &World, id: InstanceId) -> f64 {
    match w.dom.property_by_name(id, "TimeOfDay") {
        Some(Value::String(s)) => clock_hours(&s),
        _ => 14.0,
    }
}

pub(crate) fn set_lighting_hours(
    w: &mut World,
    id: InstanceId,
    hours: f64,
) -> Result<(), DomError> {
    set_named(w, id, "TimeOfDay", Value::String(time_of_day(hours)))
}

pub(crate) fn get(w: &World, id: InstanceId, name: &str) -> Option<Value> {
    let dom = &w.dom;
    if let Some(target) = view(w, id, name, SAME_AS) {
        return dom.property_by_name(id, target);
    }
    if let Some(target) = view(w, id, name, BRICK_COLOR_OF) {
        return match dom.property_by_name(id, target) {
            Some(Value::Color3(c)) => Some(Value::BrickColor(closest_brick_color(c))),
            _ => None,
        };
    }
    match name {
        "ClockTime" if dom.is_a(id, "Lighting") => {
            Some(Value::Float32(lighting_hours(w, id) as f32))
        }
        "Workspace" if id == dom.root() => Some(Value::Ref(
            dom.find_service("Workspace").map(InstanceId::to_ref),
        )),
        "Terrain" if dom.is_a(id, "Workspace") => Some(Value::Ref(
            dom.children(id)
                .iter()
                .copied()
                .find(|c| dom.class_name(*c) == "Terrain")
                .map(InstanceId::to_ref),
        )),
        "Position" | "Orientation" | "Rotation" if dom.is_a(id, "BasePart") => {
            let c = cframe(w, id);
            Some(Value::Vector3(match name {
                "Position" => Vector3(c.position),
                "Orientation" => orientation(&c),
                _ => rotation(&c),
            }))
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
    if let Some(target) = view(w, id, name, SAME_AS) {
        return Some(set_named(w, id, target, value.clone()));
    }
    if let (Some(target), Value::BrickColor(b)) = (view(w, id, name, BRICK_COLOR_OF), value) {
        return Some(set_named(w, id, target, Value::Color3(Color3::from(*b))));
    }
    match (name, value) {
        ("ClockTime", Value::Float32(h)) if w.dom.is_a(id, "Lighting") => {
            Some(set_lighting_hours(w, id, f64::from(*h)))
        }
        ("Position", Value::Vector3(p)) if w.dom.is_a(id, "BasePart") => {
            let mut c = cframe(w, id);
            c.position = p.0;
            Some(set_cframe(w, id, c))
        }
        ("Orientation", Value::Vector3(v)) if w.dom.is_a(id, "BasePart") => {
            let mut c = cframe(w, id);
            let r = |d: f32| d.to_radians();
            c.rotation = CFrame::from_orientation(r(v.x()), r(v.y()), r(v.z())).rotation;
            Some(set_cframe(w, id, c))
        }
        ("Rotation", Value::Vector3(v)) if w.dom.is_a(id, "BasePart") => {
            let mut c = cframe(w, id);
            let r = |d: f32| d.to_radians();
            c.rotation = CFrame::angles(r(v.x()), r(v.y()), r(v.z())).rotation;
            Some(set_cframe(w, id, c))
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
