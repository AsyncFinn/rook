//! SurfaceSelection: an outline around one face of its Adornee.

use rook_types::PartShape;

use super::*;

/// How wide the outline's bars are, centered on the face's edges (measured
/// in Studio 0.742, 2026-10-20: 0.4 studs on a 4-stud cube's Front and
/// Right faces, reaching 0.2 past them).
const WIDTH: f32 = 0.4;

/// One SurfaceSelection on a part of `size`: four flat bars on its
/// `TargetSurface` (Right by default), in `Color3` without light, lying a
/// hair outside the face. Parts in front hide it (observed).
pub(super) fn surface_selection(
    dom: &Dom,
    id: InstanceId,
    size: Vec3,
) -> Vec<(PartInstance, Pose)> {
    let face = dom
        .enum_property(id, "TargetSurface")
        .and_then(NormalId::from_u32)
        .unwrap_or(NormalId::Right);
    let bar = piece(dom, id, DrawShape::Part(PartShape::Block));
    if bar.transparency >= 1.0 {
        return Vec::new();
    }
    let normal = face.normal();
    let axis = normal.abs().max_position();
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let half = size * 0.5;
    let mut out = Vec::new();
    // The u bars run the whole side, corners included; the v bars stop at
    // them, so a see-through outline doesn't darken its corners.
    for (along, across) in [(u, v), (v, u)] {
        let length = if along == u {
            size[along] + WIDTH
        } else {
            size[along] - WIDTH
        };
        if length <= 0.0 {
            continue;
        }
        let mut extent = Vec3::splat(WIDTH);
        extent[axis] = 0.02;
        extent[along] = length;
        for side in [-1.0, 1.0] {
            let mut offset = normal * (half[axis] + 0.01);
            offset[across] = side * half[across];
            out.push((
                PartInstance {
                    size: extent,
                    ..bar
                },
                at(offset),
            ));
        }
    }
    out
}
