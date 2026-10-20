//! Reading physics-relevant properties by name, with Roblox's defaults for
//! missing or mistyped values.

use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, Value};

pub(crate) fn cframe(dom: &Dom, id: InstanceId, name: &str) -> CFrame {
    read(dom, id, name, Value::as_cframe).unwrap_or(CFrame::IDENTITY)
}

pub(crate) fn float(dom: &Dom, id: InstanceId, name: &str) -> f32 {
    read(dom, id, name, Value::as_number).map_or(0.0, |n| n as f32)
}

pub(crate) fn vector(dom: &Dom, id: InstanceId, name: &str) -> Vec3 {
    read(dom, id, name, Value::as_vector3).unwrap_or(Vec3::ZERO)
}

pub(crate) fn enum_value(dom: &Dom, id: InstanceId, name: &str) -> u32 {
    read(dom, id, name, Value::as_enum).unwrap_or(0)
}

/// A Bool property, `default` when it's missing.
pub(crate) fn boolean(dom: &Dom, id: InstanceId, name: &str, default: bool) -> bool {
    read(dom, id, name, Value::as_bool).unwrap_or(default)
}

fn read<T>(
    dom: &Dom,
    id: InstanceId,
    name: &str,
    f: impl FnOnce(&Value) -> Option<T>,
) -> Option<T> {
    f(&dom.property_by_name(id, name)?)
}
