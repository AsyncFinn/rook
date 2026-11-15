//! ViewportFrame: its border as a Frame's, then its own world's picture,
//! which holds its background (docs/behavior/gui/viewport-frames.md). The
//! renderer draws the picture; this asks for it at the frame's size.

use rook_dom::{Dom, InstanceId};
use rook_types::{GuiQuad, GuiViewport, Paint};

use crate::layout::{Placed, turn};

pub(in crate::draw) fn is_viewport(dom: &Dom, id: InstanceId) -> bool {
    dom.class_name(id) == "ViewportFrame"
}

/// The picture over `p`, a pixel of it for each pixel the frame covers
/// (observed: edges a pixel sharp), tinted by `ImageColor3` and faded by
/// `ImageTransparency`, background and all (measured).
pub(in crate::draw) fn emit(
    dom: &Dom,
    id: InstanceId,
    p: Placed,
    clip: [f32; 4],
    out: &mut Vec<GuiQuad>,
) {
    let alpha = 1.0 - dom.float_property(id, "ImageTransparency").unwrap_or(0.0);
    let size = p.size().abs();
    let pixels = size.ceil();
    if alpha <= 0.0 || pixels.x < 1.0 || pixels.y < 1.0 {
        return;
    }
    let tint = dom
        .color3_property(id, "ImageColor3")
        .map_or([1.0; 3], |c| [c.r, c.g, c.b]);
    out.push(GuiQuad {
        center: p.center + turn(p.rotation, (pixels - size) * 0.5),
        size: pixels,
        rotation: p.rotation.to_radians(),
        color: [tint[0], tint[1], tint[2], alpha.min(1.0)],
        clip,
        paint: Paint::Viewport(GuiViewport {
            key: id.to_ref().0,
            size: [pixels.x as u32, pixels.y as u32],
        }),
        shape: None,
        gradient: None,
    });
}
