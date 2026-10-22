//! UIStroke: a band around an object's edge, or an outline around its
//! text (docs/behavior/gui/modern.md#uistroke).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::{GuiQuad, GuiShape, Paint};

use super::gradient;
use crate::layout::Placed;

/// `id`'s first enabled UIStroke.
fn find(dom: &Dom, id: InstanceId) -> Option<InstanceId> {
    (dom.children(id).iter().copied()).find(|&c| {
        dom.class_name(c) == "UIStroke" && dom.bool_property(c, "Enabled") != Some(false)
    })
}

/// Whether the stroke goes round the text rather than the edge: Contextual
/// on a text object (Confirmed: creator-docs ApplyStrokeMode).
fn on_text(dom: &Dom, id: InstanceId, stroke: InstanceId) -> bool {
    crate::text::is_text(dom, id)
        && dom.enum_item_property(stroke, "ApplyStrokeMode") != Some("Border")
}

/// The stroke's color and opacity (measured: only its own `Transparency`
/// counts, not the object's).
fn color(dom: &Dom, stroke: InstanceId) -> [f32; 4] {
    let c = dom
        .color3_property(stroke, "Color")
        .map_or([0.0; 3], |c| [c.r, c.g, c.b]);
    let alpha = 1.0 - dom.float_property(stroke, "Transparency").unwrap_or(0.0);
    [c[0], c[1], c[2], alpha]
}

/// The stroke's width in pixels as drawn. Measured: a UIScale widens it
/// even at `StrokeSizingMode` FixedSize; ScaledSize's `Thickness` is a
/// share of the shorter side.
fn thickness(dom: &Dom, stroke: InstanceId, p: Placed) -> f32 {
    let t = dom
        .float_property(stroke, "Thickness")
        .unwrap_or(1.0)
        .max(0.0);
    match dom.enum_item_property(stroke, "StrokeSizingMode") {
        Some("ScaledSize") => t * p.size().abs().min_element(),
        _ => t * p.scale,
    }
}

/// The outline a UIStroke draws round `id`'s text instead of its text
/// stroke (measured: `TextStrokeTransparency` 0 shows only the UIStroke):
/// its color and width in pixels as drawn.
pub(super) fn text_outline(dom: &Dom, id: InstanceId, p: Placed) -> Option<([f32; 4], f32)> {
    let stroke = find(dom, id).filter(|&s| on_text(dom, id, s))?;
    Some((color(dom, stroke), thickness(dom, stroke, p)))
}

/// The band round `id`'s edge, over what it draws and under its
/// children (measured), following its UICorner's `radii`. Measured: Outer
/// (the default) lies outside the edge, Center across it, Inner inside;
/// without a UICorner, the outside corners are rounded by its width
/// (`LineJoinMode` Round), cut (Bevel) or square (Miter); with one, they
/// follow it, the outside edge's radius its radius plus the band's width
/// outside the edge.
pub(super) fn emit(
    dom: &Dom,
    id: InstanceId,
    p: Placed,
    radii: Option<[f32; 4]>,
    clip: [f32; 4],
    out: &mut Vec<GuiQuad>,
) {
    let Some(stroke) = find(dom, id).filter(|&s| !on_text(dom, id, s)) else {
        return;
    };
    let width = thickness(dom, stroke, p);
    let color = color(dom, stroke);
    if width <= 0.0 || color[3] <= 0.0 {
        return;
    }
    let outside = match dom.enum_item_property(stroke, "BorderStrokePosition") {
        Some("Center") => width * 0.5,
        Some("Inner") => 0.0,
        _ => width,
    };
    // Inference: BorderOffset moves the band out by its offset in pixels
    // and its scale of the shorter side.
    let offset = dom
        .udim_property(stroke, "BorderOffset")
        .map_or(0.0, |u| u.resolve(p.local.abs().min_element()) * p.scale);
    let grow = outside + offset;
    let join = dom.enum_item_property(stroke, "LineJoinMode");
    let (radii, bevel) = match radii {
        Some(r) => (r.map(|r| r + grow), false),
        None => match join {
            Some("Miter") => ([0.0; 4], false),
            Some("Bevel") => ([grow; 4], true),
            _ => ([grow; 4], false),
        },
    };
    let size = p.size().abs() + Vec2::splat(2.0 * grow);
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    out.push(GuiQuad {
        center: p.center,
        size,
        rotation: p.rotation.to_radians(),
        color,
        clip,
        paint: Paint::Solid,
        shape: Some(GuiShape {
            center: p.center,
            size,
            radii,
            ring: width,
            bevel,
        }),
        gradient: gradient::of(dom, stroke, p, size),
    });
}
