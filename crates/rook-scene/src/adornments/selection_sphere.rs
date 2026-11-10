//! SelectionSphere: a ring facing the camera around its Adornee.

use super::*;

/// One SelectionSphere on a part of `size`: its ring, and its fill inside.
///
/// Measured in Studio 0.742 (2026-10-20, cubes of 1, 2, 4 and 10 studs and
/// a 4 × 2 × 6 box, seen from in front and from 45° above): a flat ring
/// facing the camera from any side, about 1 stud wide, from
/// `s/2 - 0.8` to `s/2 + 0.2` studs out, where `s` is the part's smallest
/// side (no hole below about 1.6 studs). It draws in exactly `Color3`,
/// past the tone curve; `SurfaceColor3` at `SurfaceTransparency` (1 by
/// default) fills the hole through the curve. Parts in front hide both.
pub(super) fn selection_sphere(dom: &Dom, id: InstanceId, size: Vec3) -> Vec<(PartInstance, Pose)> {
    let half = size.abs().min_element() * 0.5;
    let (inner, outer) = ((half - 0.8).max(0.0), half + 0.2);
    let mut out = Vec::new();
    let ring = PartInstance {
        size: Vec3::new(2.0 * outer, 2.0 * outer, 1.0),
        light: Light::Exact,
        faces_camera: true,
        ..piece(
            dom,
            id,
            DrawShape::Adorn(AdornShape::Ring {
                hole: AdornShape::hole(inner, outer),
            }),
        )
    };
    if ring.transparency < 1.0 {
        out.push((ring, Pose::default()));
    }
    let fill = dom.float_property(id, "SurfaceTransparency").unwrap_or(1.0);
    if fill < 1.0 && inner > 0.0 {
        // Studio's fill reaches s/2, under the ring (observed: a 0.5 fill
        // tints the ring's inner part); Rook fills the hole only.
        out.push((
            PartInstance {
                color: color(dom, id, "SurfaceColor3"),
                transparency: fill,
                size: Vec3::new(2.0 * inner, 2.0 * inner, 1.0),
                light: Light::Unlit,
                shape: DrawShape::Adorn(AdornShape::Ring { hole: 0 }),
                ..ring
            },
            Pose::default(),
        ));
    }
    out
}
