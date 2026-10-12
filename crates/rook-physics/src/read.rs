//! Reading physics-relevant properties by name, with Roblox's defaults for
//! missing or mistyped values.

use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, Value};

pub(crate) fn cframe(dom: &Dom, id: InstanceId, name: &str) -> CFrame {
    match dom.property_by_name(id, name) {
        Some(Value::CFrame(c)) => c,
        _ => CFrame::IDENTITY,
    }
}

pub(crate) fn float(dom: &Dom, id: InstanceId, name: &str) -> f32 {
    match dom.property_by_name(id, name) {
        Some(Value::Float32(f)) => f,
        Some(Value::Float64(f)) => f as f32,
        Some(Value::Int32(n)) => n as f32,
        _ => 0.0,
    }
}

pub(crate) fn vector(dom: &Dom, id: InstanceId, name: &str) -> Vec3 {
    match dom.property_by_name(id, name) {
        Some(Value::Vector3(v)) => v.0,
        _ => Vec3::ZERO,
    }
}

pub(crate) fn enum_value(dom: &Dom, id: InstanceId, name: &str) -> u32 {
    match dom.property_by_name(id, name) {
        Some(Value::Enum(n)) => n,
        _ => 0,
    }
}

/// A Bool property, `default` when it's missing.
pub(crate) fn boolean(dom: &Dom, id: InstanceId, name: &str, default: bool) -> bool {
    match dom.property_by_name(id, name) {
        Some(Value::Bool(b)) => b,
        _ => default,
    }
}
