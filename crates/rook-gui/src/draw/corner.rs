//! UICorner: an object's background, image and text cut to a rectangle
//! with rounded corners (docs/behavior/gui/modern.md#uicorner).

use rook_dom::{Dom, InstanceId};
use rook_types::GuiShape;

use crate::layout::Placed;

/// The corners' properties, in [`GuiShape::radii`]'s order.
const CORNERS: [&str; 4] = [
    "TopLeftRadius",
    "TopRightRadius",
    "BottomRightRadius",
    "BottomLeftRadius",
];

/// The radii of `id`'s first UICorner, in pixels as drawn. Measured: no
/// radius goes past half the shorter side (a scale of 0.5 and an offset of
/// 60 both make a 160 × 80 frame a pill). Inference: scale is against the
/// shorter side, and UIScales grow the offset.
pub(super) fn radii(dom: &Dom, id: InstanceId, p: Placed) -> Option<[f32; 4]> {
    let corner = dom.find_first_child_of_class(id, "UICorner")?;
    let short = p.local.x.abs().min(p.local.y.abs());
    Some(CORNERS.map(|name| {
        let r = dom
            .udim_property(corner, name)
            .map_or(8.0, |u| u.resolve(short));
        r.clamp(0.0, short * 0.5) * p.scale
    }))
}

/// `p`'s rectangle with corners of `radii`.
pub(super) fn shape(p: Placed, radii: [f32; 4]) -> GuiShape {
    GuiShape {
        center: p.center,
        size: p.size().abs(),
        radii,
        ring: 0.0,
        bevel: false,
    }
}
