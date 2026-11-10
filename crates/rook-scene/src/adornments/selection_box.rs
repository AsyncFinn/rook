//! SelectionBox: the edges and fill of its Adornee's box.

use rook_types::PartShape;

use super::*;

/// One SelectionBox on a part of `size`, as its fill and 12 edge bars.
///
/// Observed in Studio 0.742 (2026-10-20): each edge is a bar
/// 2 × `LineThickness` across, centered on the part's edge, in `Color3`
/// without light; `SurfaceColor3` fills the box at `SurfaceTransparency`
/// (1 by default: no fill); parts in front hide both.
pub(super) fn selection_box(dom: &Dom, id: InstanceId, size: Vec3) -> Vec<(PartInstance, Pose)> {
    let thickness = dom
        .float_property(id, "LineThickness")
        .unwrap_or(0.15)
        .max(0.0);
    let line = piece(dom, id, DrawShape::Part(PartShape::Block));
    let mut out = Vec::new();
    let fill = dom.float_property(id, "SurfaceTransparency").unwrap_or(1.0);
    if fill < 1.0 {
        out.push((
            PartInstance {
                color: color(dom, id, "SurfaceColor3"),
                transparency: fill,
                // A hair outside the part's faces, so the two don't fight.
                size: size + Vec3::splat(0.02),
                ..line
            },
            Pose::default(),
        ));
    }
    if thickness > 0.0 && line.transparency < 1.0 {
        let half = size * 0.5;
        let width = 2.0 * thickness;
        // X bars run the whole length, corners included; the Y and Z bars
        // stop at them, so a see-through box doesn't darken its corners.
        for axis in 0..3 {
            let length = if axis == 0 {
                size.x + width
            } else {
                size[axis] - width
            };
            if length <= 0.0 {
                continue;
            }
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let mut bar = Vec3::splat(width);
            bar[axis] = length;
            for (su, sv) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
                let mut offset = Vec3::ZERO;
                offset[u] = su * half[u];
                offset[v] = sv * half[v];
                out.push((PartInstance { size: bar, ..line }, at(offset)));
            }
        }
    }
    out
}
