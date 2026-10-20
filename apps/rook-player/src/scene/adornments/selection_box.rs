//! SelectionBox: the edges and fill of its Adornee's box.

use rook_types::PartShape;

use super::*;

/// The default `Color3`, (13, 105, 172) (observed).
const DEFAULT_COLOR: [f32; 3] = [13.0 / 255.0, 105.0 / 255.0, 172.0 / 255.0];

/// The SelectionBoxes adorning part `id`, each as its 12 edge bars and its
/// fill, with their poses in the part's axes.
///
/// Observed in Studio 0.742 (2026-10-20): a SelectionBox draws only on its
/// `Adornee` (one merely inside a part draws nothing), wherever it is in
/// Workspace; each edge is a bar 2 × `LineThickness` across, centred on
/// the part's edge, in `Color3` without light; `SurfaceColor3` fills the
/// box at `SurfaceTransparency` (1 by default: no fill); parts in front
/// hide both.
pub(in crate::scene) fn selection_boxes(dom: &Dom, id: InstanceId) -> Vec<(PartInstance, Pose)> {
    let (Some(size), Some(cf)) = (
        dom.vector3_property(id, "Size"),
        dom.cframe_property(id, "CFrame"),
    ) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for &(holder, _) in dom.referrers(id) {
        if dom.class_name(holder) != "SelectionBox"
            || dom.ref_property(holder, "Adornee") != Some(id)
            || !shown(dom, holder)
        {
            continue;
        }
        if out.iter().any(|(_, _, h)| *h == holder) {
            continue;
        }
        for (part, pose) in selection_box(dom, holder, size) {
            out.push((part, pose, holder));
        }
    }
    out.into_iter()
        .map(|(mut part, pose, _)| {
            part.position = cf.position + cf.rotation * pose.offset;
            part.rotation = cf.rotation * pose.turn;
            (part, pose)
        })
        .collect()
}

/// One SelectionBox on a part of `size`.
fn selection_box(dom: &Dom, id: InstanceId, size: Vec3) -> Vec<(PartInstance, Pose)> {
    let thickness = dom
        .float_property(id, "LineThickness")
        .unwrap_or(0.15)
        .max(0.0);
    let line = PartInstance {
        color: dom
            .color3_property(id, "Color3")
            .map(Color3::to_array)
            .unwrap_or(DEFAULT_COLOR),
        transparency: dom.float_property(id, "Transparency").unwrap_or(0.0),
        cast_shadow: false,
        unlit: true,
        ..PartInstance::plain(
            DrawShape::Part(PartShape::Block),
            Vec3::ZERO,
            Mat3::IDENTITY,
            Vec3::ONE,
        )
    };
    let mut out = Vec::new();
    let fill = dom.float_property(id, "SurfaceTransparency").unwrap_or(1.0);
    if fill < 1.0 {
        out.push((
            PartInstance {
                color: dom
                    .color3_property(id, "SurfaceColor3")
                    .map(Color3::to_array)
                    .unwrap_or(DEFAULT_COLOR),
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
                out.push((
                    PartInstance { size: bar, ..line },
                    Pose {
                        offset,
                        turn: Mat3::IDENTITY,
                    },
                ));
            }
        }
    }
    out
}
