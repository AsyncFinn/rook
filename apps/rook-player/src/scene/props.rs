//! Reading the properties a scene is built from.

use super::*;

pub(super) fn number(dom: &Dom, id: InstanceId, name: &str) -> Option<f32> {
    dom.property_by_name(id, name)?
        .as_number()
        .map(|n| n as f32)
}

pub(super) fn color(dom: &Dom, id: InstanceId, name: &str) -> Option<[f32; 3]> {
    let c = dom.property_by_name(id, name)?.as_color3()?;
    Some([c.r, c.g, c.b])
}

pub(super) fn boolean(dom: &Dom, id: InstanceId, name: &str) -> Option<bool> {
    dom.property_by_name(id, name)?.as_bool()
}

pub(super) fn vector3(dom: &Dom, id: InstanceId, name: &str) -> Option<Vec3> {
    dom.property_by_name(id, name)?.as_vector3()
}

/// A NumberSequence as a curve with its keypoints' envelopes.
pub(super) fn number_sequence(dom: &Dom, id: InstanceId, name: &str) -> Option<Sequence> {
    let Value::NumberSequence(seq) = dom.property_by_name(id, name)? else {
        return None;
    };
    if seq.keypoints.is_empty() {
        return None;
    }
    let curve = Curve(seq.keypoints.iter().map(|k| (k.time, k.value)).collect());
    let envelope: Vec<f32> = seq.keypoints.iter().map(|k| k.envelope).collect();
    let envelope = if envelope.iter().all(|&e| e == 0.0) {
        Vec::new()
    } else {
        envelope
    };
    Some(Sequence { curve, envelope })
}

/// A ColorSequence as a curve of sRGB colours.
pub(super) fn color_sequence(dom: &Dom, id: InstanceId, name: &str) -> Option<Curve<[f32; 3]>> {
    let Value::ColorSequence(seq) = dom.property_by_name(id, name)? else {
        return None;
    };
    let points: Vec<_> = seq
        .keypoints
        .iter()
        .map(|k| (k.time, [k.color.r, k.color.g, k.color.b]))
        .collect();
    (!points.is_empty()).then_some(Curve(points))
}

/// A NumberRange's minimum and maximum.
pub(super) fn number_range(dom: &Dom, id: InstanceId, name: &str) -> Option<[f32; 2]> {
    let Value::NumberRange(r) = dom.property_by_name(id, name)? else {
        return None;
    };
    Some([r.min, r.max])
}

pub(super) fn vector2(dom: &Dom, id: InstanceId, name: &str) -> Option<Vec2> {
    let Value::Vector2(v) = dom.property_by_name(id, name)? else {
        return None;
    };
    Some(Vec2::new(v.x(), v.y()))
}

pub(super) fn enum_value(dom: &Dom, id: InstanceId, name: &str) -> Option<u32> {
    dom.property_by_name(id, name)?.as_enum()
}

/// The first non-empty content URL among `names` (files store the
/// `Content` property; some old files saved the legacy name).
pub(super) fn content_url(dom: &Dom, id: InstanceId, names: &[&str]) -> String {
    names
        .iter()
        .find_map(|name| {
            dom.property_by_name(id, name)?
                .content_url()
                .filter(|u| !u.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

/// The mesh child that decides how a part looks: of several, the one added
/// last (observed: a part with a SpecialMesh Sphere and a CylinderMesh drew
/// whichever was parented second). With its kind and `MeshId`.
pub(super) fn part_mesh(dom: &Dom, id: InstanceId) -> Option<(MeshKind, InstanceId, String)> {
    dom.children(id).iter().rev().find_map(|&child| {
        let mesh_type = enum_value(dom, child, "MeshType");
        let mesh_id = mesh_id(dom, child);
        let kind = MeshKind::of(dom.class_name(child), mesh_type, &mesh_id)?;
        Some((kind, child, mesh_id))
    })
}

/// A mesh's or MeshPart's `MeshId`. Files store it in `MeshContent` (`MeshId`
/// is a view of it); some 2016 MeshParts saved it as `MeshID`.
pub(super) fn mesh_id(dom: &Dom, id: InstanceId) -> String {
    content_url(dom, id, &["MeshContent", "MeshID"])
}

/// A mesh's `TextureId` or a MeshPart's `TextureID`.
pub(super) fn texture_id(dom: &Dom, id: InstanceId) -> String {
    content_url(dom, id, &["TextureContent", "TextureID"])
}
