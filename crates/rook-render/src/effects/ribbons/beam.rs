//! A Beam's strip: its Bézier curve between its Attachments.

use super::*;

/// The most pieces a Beam is drawn in (`Segments`).
const MAX_SEGMENTS: u32 = 1000;

/// A Beam's strip: its Bézier curve in `segments` pieces.
pub(super) fn beam<'a>(b: &'a Beam, camera: Vec3, time: f32) -> Option<Strip<'a>> {
    if !b.enabled {
        return None;
    }
    let [a, z] = b.ends;
    let p0 = a.position;
    let p3 = z.position;
    let p1 = p0 + a.rotation.x_axis * b.curve[0];
    let p2 = p3 - z.rotation.x_axis * b.curve[1];
    let n = b.segments.clamp(1, MAX_SEGMENTS);
    let points: Vec<(f32, Vec3, Vec3)> = (0..=n)
        .map(|i| {
            let s = i as f32 / n as f32;
            let r = 1.0 - s;
            let at = p0 * (r * r * r)
                + p1 * (3.0 * r * r * s)
                + p2 * (3.0 * r * s * s)
                + p3 * (s * s * s);
            let tangent =
                (p1 - p0) * (3.0 * r * r) + (p2 - p1) * (6.0 * r * s) + (p3 - p2) * (3.0 * s * s);
            (s, at, tangent)
        })
        .collect();
    let mut along = vec![0.0];
    for pair in points.windows(2) {
        along.push(along.last().unwrap() + pair[0].1.distance(pair[1].1));
    }
    if along[n as usize] <= 0.0 && b.width == [0.0, 0.0] {
        return None;
    }
    let look = &b.look;
    let length = look.texture_length.max(1e-3);
    // One cycle a second at TextureSpeed 1, from Attachment0 towards
    // Attachment1; SetTextureOffset goes the other way (creator-docs).
    let scroll = time * b.texture_speed - b.texture_offset;
    let edges = points
        .iter()
        .zip(&along)
        .map(|(&(s, at, tangent), &d)| {
            let width = b.width[0] + (b.width[1] - b.width[0]) * s;
            let to_camera = (camera - at).normalize_or_zero();
            let across = if look.face_camera {
                across_view(tangent, at, camera)
            } else {
                // Along the Attachments' up however the curve runs
                // (observed: a steep end stays an upright band).
                a.rotation
                    .y_axis
                    .lerp(z.rotation.y_axis, s)
                    .normalize_or(Vec3::Y)
            };
            let at = at + to_camera * b.z_offset;
            let u = match look.texture_mode {
                TextureMode::Stretch => s * look.texture_length,
                TextureMode::Wrap | TextureMode::Static => d / length,
            } - scroll;
            Edge {
                sides: [at - across * width * 0.5, at + across * width * 0.5],
                u,
                color: look.color.at(s),
                alpha: look.alpha.at(s, |_| 0.0).clamp(0.0, 1.0),
            }
        })
        .collect();
    Some(Strip { look, edges })
}
