//! A GUI object's background and the border around it.

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::{GuiQuad, Paint};

use crate::layout::{Placed, turn};

/// Inference: the border takes the background's transparency and doesn't
/// overlap it, and UIScales thicken it.
pub(super) fn emit(dom: &Dom, id: InstanceId, p: Placed, clip: [f32; 4], out: &mut Vec<GuiQuad>) {
    let alpha = 1.0
        - dom
            .float_property(id, "BackgroundTransparency")
            .unwrap_or(0.0);
    if alpha <= 0.0 {
        return;
    }
    let size = p.size();
    let border = dom.number_property(id, "BorderSizePixel").unwrap_or(0.0) as f32 * p.scale;
    // Confirmed (creator-docs BorderMode): Outline outside the object,
    // Middle across its edge, Inset inside.
    let (inset, outset) = match dom.enum_item_property(id, "BorderMode") {
        Some("Middle") => (border * 0.5, border * 0.5),
        Some("Inset") => (border, 0.0),
        _ => (0.0, border),
    };
    let quad = |offset: Vec2, extent: Vec2, color: [f32; 3]| GuiQuad {
        center: p.center + turn(p.rotation, offset),
        size: extent,
        rotation: p.rotation.to_radians(),
        color: [color[0], color[1], color[2], alpha],
        clip,
        paint: Paint::Solid,
    };
    if border > 0.0 {
        let color = dom
            .color3_property(id, "BorderColor3")
            .map_or([0.0; 3], |c| [c.r, c.g, c.b]);
        let outer = size.abs() + Vec2::splat(2.0 * outset);
        let thick = border;
        let half = outer * 0.5 - thick * 0.5;
        out.push(quad(
            Vec2::new(0.0, -half.y),
            Vec2::new(outer.x, thick),
            color,
        ));
        out.push(quad(
            Vec2::new(0.0, half.y),
            Vec2::new(outer.x, thick),
            color,
        ));
        let side = Vec2::new(thick, outer.y - 2.0 * thick);
        if side.y > 0.0 {
            out.push(quad(Vec2::new(-half.x, 0.0), side, color));
            out.push(quad(Vec2::new(half.x, 0.0), side, color));
        }
    }
    let inner = size.abs() - Vec2::splat(2.0 * inset);
    if inner.x > 0.0 && inner.y > 0.0 {
        let color = dom
            .color3_property(id, "BackgroundColor3")
            .map_or([1.0; 3], |c| [c.r, c.g, c.b]);
        out.push(quad(Vec2::ZERO, inner, tint(dom, id, color)));
    }
}

/// AutoButtonColor's tint on a button's background. Measured: hovered,
/// the color times 0.7; pressed, mixed 30% towards white
/// (docs/behavior/gui/input.md#buttons).
fn tint(dom: &Dom, id: InstanceId, color: [f32; 3]) -> [f32; 3] {
    if !dom.is_a(id, "GuiButton") || dom.bool_property(id, "AutoButtonColor") == Some(false) {
        return color;
    }
    match dom.enum_item_property(id, "GuiState") {
        Some("Hover") => color.map(|c| c * 0.7),
        Some("Press") => color.map(|c| c + (1.0 - c) * 0.3),
        _ => color,
    }
}
