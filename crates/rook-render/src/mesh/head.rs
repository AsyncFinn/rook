//! Rook's own head mesh, drawn for the Head MeshType when the user's
//! install has none.

use super::*;

/// The head's radius and half height (about 1.2 studs across and tall).
pub(super) const HEAD_RADIUS: f32 = 0.6012;
/// Around the Y axis: the first at 5 degrees from +X towards +Z, then every
/// 10 degrees.
const HEAD_SEGMENTS: u32 = 36;
/// Steps in each rounded edge, 22.5 degrees apart.
const HEAD_EDGE_STEPS: u32 = 4;

/// The head's UVs: segments facing within 45 degrees of -Z (the front
/// quarter, from 225 to 315 degrees) map the image straight on, across
/// the head's 2R x 2R front; every other vertex has UV (0, 0), so a face
/// decal's top-left pixel covers the back and sides.
fn head_uv(front: bool, position: [f32; 3]) -> [f32; 2] {
    if !front {
        return [0.0; 2];
    }
    let span = 2.0 * HEAD_RADIUS;
    [0.5 - position[0] / span, 0.5 - position[1] / span]
}

/// Rook's own head, built from measurements of the `head.mesh` in the
/// user's Roblox install (the Head MeshType draws that file): a solid of
/// revolution with rounded rims and its front quarter's UVs projected
/// straight on. The measured facts are in `docs/behavior/meshes/built-in-shapes.md`; the
/// user's own install can supply the real file for [`DrawShape::Head`].
pub(super) fn head() -> MeshData {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let mut m = MeshData::default();
    let r = HEAD_RADIUS;
    // The profile from the bottom rim to the top rim: (radius, y, normal
    // angle above the horizontal).
    let mut profile = Vec::new();
    for (center_y, from) in [(-r / 2.0, -FRAC_PI_2), (r / 2.0, 0.0)] {
        for i in 0..=HEAD_EDGE_STEPS {
            let a = from + i as f32 / HEAD_EDGE_STEPS as f32 * FRAC_PI_2;
            profile.push((r / 2.0 + r / 2.0 * a.cos(), center_y + r / 2.0 * a.sin(), a));
        }
    }
    let around = |k: u32| (5.0f32.to_radians() + k as f32 / HEAD_SEGMENTS as f32 * TAU).sin_cos();
    // Segments 22 to 30 run from 225 to 315 degrees: the front quarter.
    let is_front = |k: u32| (22..31).contains(&k);
    let vertex = |&(radius, y, a): &(f32, f32, f32), k: u32, front: bool| {
        let (sin, cos) = around(k);
        let position = [radius * cos, y, radius * sin];
        Vertex::plain(
            position,
            [a.cos() * cos, a.sin(), a.cos() * sin],
            head_uv(front, position),
        )
    };
    // Each segment has its own vertices, so the front quarter's edges are
    // seams; the normals stay smooth.
    for k in 0..HEAD_SEGMENTS {
        let front = is_front(k);
        let base = m.vertices.len() as u32;
        for ring in &profile {
            m.vertices.push(vertex(ring, k, front));
            m.vertices.push(vertex(ring, k + 1, front));
        }
        for ring in 0..profile.len() as u32 - 1 {
            let a = base + ring * 2;
            let b = a + 2;
            m.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
        // Caps: a fan triangle from the center, sharing the rims' normals
        // (straight up and down there).
        for (ring, ny) in [(0u32, -1.0f32), (profile.len() as u32 - 1, 1.0)] {
            let center = m.vertices.len() as u32;
            let position = [0.0, ny * r, 0.0];
            m.vertices.push(Vertex::plain(
                position,
                [0.0, ny, 0.0],
                head_uv(front, position),
            ));
            let (a, b) = (base + ring * 2, base + ring * 2 + 1);
            let tri = if ny < 0.0 {
                [center, a, b]
            } else {
                [center, b, a]
            };
            m.indices.extend_from_slice(&tri);
        }
    }
    m
}
