//! CanvasGroup: the group and what it holds drawn apart into a texture,
//! then laid over as one, faded by `GroupTransparency`, tinted by
//! `GroupColor3`, cut by its UICorner and colored by its UIGradient
//! (docs/behavior/gui/modern.md#canvasgroup).

use std::sync::Arc;

use glam::Vec2;
use rook_types::{GuiGroup, GuiQuad, Paint};

use super::{Draw, corner, gradient, stroke, touches};
use crate::layout::turn;
use crate::order::Stacked;

/// Draw group `d` holding `held` (in stack order). Inference: the texture
/// has a pixel for each pixel the group covers as drawn, and what the
/// group holds is clipped to it.
pub(super) fn emit(draw: &mut Draw<'_, '_>, d: &Stacked, held: &[Stacked]) {
    let p = d.placed;
    if !touches(p, draw.canvas) {
        return;
    }
    let size = p.size().abs();
    let pixels = size.ceil();
    if pixels.x < 1.0 || pixels.y < 1.0 {
        return;
    }
    let mut inside = Vec::new();
    std::mem::swap(draw.out, &mut inside);
    let canvas = std::mem::replace(&mut draw.canvas, GuiQuad::NO_CLIP);
    let radii = draw.own(d, false);
    draw.emit_all(held);
    draw.canvas = canvas;
    std::mem::swap(draw.out, &mut inside);
    let turned = p.rotation.to_radians();
    let local = |v: Vec2| turn(-p.rotation, v - p.center) + size * 0.5;
    for q in &mut inside {
        q.center = local(q.center);
        q.rotation -= turned;
        q.clip = if p.rotation == 0.0 {
            let shift = p.center - size * 0.5;
            [
                q.clip[0] - shift.x,
                q.clip[1] - shift.y,
                q.clip[2] - shift.x,
                q.clip[3] - shift.y,
            ]
        } else {
            GuiQuad::NO_CLIP
        };
        if let Some(s) = &mut q.shape {
            s.center = local(s.center);
        }
        if let Some(g) = &mut q.gradient {
            g.origin = local(g.origin);
            g.step = turn(-p.rotation, g.step);
        }
    }
    let dom = draw.dom;
    let tint = dom
        .color3_property(d.id, "GroupColor3")
        .map_or([1.0; 3], |c| [c.r, c.g, c.b]);
    let alpha = 1.0 - dom.float_property(d.id, "GroupTransparency").unwrap_or(0.0);
    if alpha > 0.0 {
        draw.out.push(GuiQuad {
            center: p.center + turn(p.rotation, (pixels - size) * 0.5),
            size: pixels,
            rotation: turned,
            color: [tint[0], tint[1], tint[2], alpha],
            clip: d.clip,
            paint: Paint::Group(Arc::new(GuiGroup {
                key: d.id.to_ref().0,
                size: [pixels.x as u32, pixels.y as u32],
                quads: inside,
            })),
            shape: radii.map(|r| corner::shape(p, r)),
            gradient: gradient::of(dom, d.id, p, size),
        });
    }
    stroke::emit(dom, d.id, p, radii, d.clip, draw.out);
}
