//! The focused TextBox's selection, behind its text, and its caret, over
//! it, from `CursorPosition` and `SelectionStart` (1-based, −1 for none).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::{GuiQuad, Paint};

use crate::layout::Placed;
use crate::text::TextBlock;

/// Inference: Roblox's selection is a translucent blue.
const SELECTION: [f32; 4] = [0.2, 0.45, 1.0, 0.5];

/// The character before which a 1-based position stands, if it's set.
fn index(dom: &Dom, id: InstanceId, name: &str) -> Option<usize> {
    let n = dom.number_property(id, name)?;
    (n >= 1.0).then(|| n as usize - 1)
}

fn rect(p: Placed, min: Vec2, max: Vec2, color: [f32; 4], clip: [f32; 4]) -> GuiQuad {
    GuiQuad {
        center: p.screen_point((min + max) * 0.5),
        size: (max - min) * p.scale,
        rotation: p.rotation.to_radians(),
        color,
        clip,
        paint: Paint::Solid,
        shape: None,
        gradient: None,
    }
}

/// The selection, drawn before the text.
pub(super) fn selection(
    dom: &Dom,
    id: InstanceId,
    block: &TextBlock,
    p: Placed,
    clip: [f32; 4],
    out: &mut Vec<GuiQuad>,
) {
    let (Some(a), Some(b)) = (
        index(dom, id, "CursorPosition"),
        index(dom, id, "SelectionStart"),
    ) else {
        return;
    };
    let (lo, hi) = (a.min(b), a.max(b));
    for line in &block.carets {
        let inside: Vec<f32> = (line.stops.iter())
            .filter(|s| (lo..=hi).contains(&s.0))
            .map(|s| s.1)
            .collect();
        let (Some(&left), Some(&right)) = (inside.first(), inside.last()) else {
            continue;
        };
        if right > left {
            let (min, max) = (Vec2::new(left, line.top), Vec2::new(right, line.bottom));
            out.push(rect(p, min, max, SELECTION, clip));
        }
    }
}

/// The caret, drawn over the text in its color. Inference: a pixel wide,
/// as tall as the line, not blinking.
pub(super) fn caret(
    dom: &Dom,
    id: InstanceId,
    block: &TextBlock,
    p: Placed,
    clip: [f32; 4],
    out: &mut Vec<GuiQuad>,
) {
    let Some((top, height)) = index(dom, id, "CursorPosition").and_then(|i| block.caret(i)) else {
        return;
    };
    let c = dom
        .color3_property(id, "TextColor3")
        .map_or([0.0; 3], |c| [c.r, c.g, c.b]);
    let min = top - Vec2::new(0.5, 0.0);
    let max = top + Vec2::new(0.5, height);
    out.push(rect(p, min, max, [c[0], c[1], c[2], 1.0], clip));
}
