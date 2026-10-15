//! Attenuation curves (`SetDistanceAttenuation`, `SetAngleAttenuation`),
//! kept in the emitter's or listener's `DistanceAttenuation` and
//! `AngleAttenuation` properties so they replicate as Studio's do.
//!
//! Studio's encoding of those BinaryStrings isn't known (the default is
//! one zero byte), so Rook writes its own: a little-endian `u16` count,
//! then each point as two little-endian `f32`s, distance (or angle) and
//! volume. Anything else, Studio's default included, reads as no curve
//! (Approximation: a curve saved in a file by Studio is lost; no archive
//! place has one).

use rbx_types::BinaryString;
use rook_dom::{Dom, DomError, InstanceId};
use rook_types::Value;

/// The curve in `property`, sorted by distance; empty for none.
pub fn read_curve(dom: &Dom, id: InstanceId, property: &str) -> Vec<(f32, f32)> {
    let Some(Value::BinaryString(b)) = dom.property_by_name(id, property) else {
        return Vec::new();
    };
    let bytes: &[u8] = b.as_ref();
    let Some((count, rest)) = bytes.split_first_chunk::<2>() else {
        return Vec::new();
    };
    let count = usize::from(u16::from_le_bytes(*count));
    if rest.len() != count * 8 {
        return Vec::new();
    }
    let f = |c: &[u8]| f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
    rest.as_chunks::<8>()
        .0
        .iter()
        .map(|c| (f(&c[..4]), f(&c[4..])))
        .collect()
}

/// Store `points`: sorted, volumes kept within 0 to 1 (observed: 2 read
/// back as 1 and -1 as 0).
pub fn write_curve(
    dom: &mut Dom,
    id: InstanceId,
    property: &str,
    mut points: Vec<(f32, f32)>,
) -> Result<(), DomError> {
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut bytes = Vec::with_capacity(2 + points.len() * 8);
    bytes.extend_from_slice(&(points.len() as u16).to_le_bytes());
    for (d, v) in points {
        bytes.extend_from_slice(&d.to_le_bytes());
        bytes.extend_from_slice(&v.clamp(0.0, 1.0).to_le_bytes());
    }
    dom.set_property_by_name(id, property, Value::BinaryString(BinaryString::from(bytes)))
}
