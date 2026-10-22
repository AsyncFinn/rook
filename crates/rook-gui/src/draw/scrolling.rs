//! A ScrollingFrame's scroll bars, over its background and under what it
//! holds: the line its border draws between each bar and the window, and
//! each bar's thumb (docs/behavior/gui/scrolling.md#drawing).

use std::f32::consts::FRAC_PI_2;

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::{GuiQuad, Paint};

use super::image::{Look, Piece, quad};
use crate::images::Images;
use crate::layout::{Placed, Scroll};

/// The thumb's top, middle and bottom images.
const PARTS: [&str; 3] = ["TopImageContent", "MidImageContent", "BottomImageContent"];

pub(super) fn emit(
    dom: &Dom,
    id: InstanceId,
    p: Placed,
    s: &Scroll,
    clip: [f32; 4],
    images: &mut Images,
    out: &mut Vec<GuiQuad>,
) {
    let size = p.local;
    let (t, b) = (s.thickness, s.border);
    let alpha = 1.0
        - dom
            .float_property(id, "BackgroundTransparency")
            .unwrap_or(0.0);
    if b > 0.0 && alpha > 0.0 {
        let c = dom
            .color3_property(id, "BorderColor3")
            .map_or([0.0; 3], |c| [c.r, c.g, c.b]);
        let color = [c[0], c[1], c[2], alpha];
        // Measured: each line crosses the whole frame.
        if s.vertical {
            let x = if s.left { t } else { s.window.x };
            out.push(line(
                p,
                [Vec2::new(x, 0.0), Vec2::new(x + b, size.y)],
                color,
                clip,
            ));
        }
        if s.horizontal {
            let y = s.window.y;
            out.push(line(
                p,
                [Vec2::new(0.0, y), Vec2::new(size.x, y + b)],
                color,
                clip,
            ));
        }
    }
    let alpha = 1.0
        - dom
            .float_property(id, "ScrollBarImageTransparency")
            .unwrap_or(0.0);
    if alpha <= 0.0 {
        return;
    }
    let c = dom
        .color3_property(id, "ScrollBarImageColor3")
        .map_or([1.0; 3], |c| [c.r, c.g, c.b]);
    let color = [c[0], c[1], c[2], alpha];
    let parts = PARTS.map(|name| {
        let url = dom.content_property(id, name).unwrap_or_default();
        images.get(&url).map(|image| Look {
            image,
            color,
            pixelated: false,
            tiled: false,
        })
    });
    let [across, down] = s.tracks();
    if let Some(track) = down {
        let (start, length) = s.thumb(1);
        for (piece, look) in pieces(start, length, t).into_iter().zip(parts) {
            let to = piece.map(|v| track + Vec2::new(v.y, v.x));
            out.extend(look.and_then(|look| part(p, to, look, clip, false)));
        }
    }
    if let Some(track) = across {
        let (start, length) = s.thumb(0);
        for (piece, look) in pieces(start, length, t).into_iter().zip(parts) {
            let to = piece.map(|v| track + v);
            out.extend(look.and_then(|look| part(p, to, look, clip, true)));
        }
    }
}

/// The thumb's top, middle and bottom pieces, along the track (x) and
/// across it (y). Measured: the ends are `thickness` square, whatever
/// their images' shape, and the middle stretches between them.
fn pieces(start: f32, length: f32, thickness: f32) -> [[Vec2; 2]; 3] {
    let at = |a: f32, b: f32| [Vec2::new(start + a, 0.0), Vec2::new(start + b, thickness)];
    [
        at(0.0, thickness),
        at(thickness, length - thickness),
        at(length - thickness, length),
    ]
}

/// One piece of a thumb at `to` in the frame. Measured: the horizontal
/// bar's images are the vertical bar's turned a quarter left, the top
/// image at the left end.
fn part(p: Placed, to: [Vec2; 2], look: Look, clip: [f32; 4], turned: bool) -> Option<GuiQuad> {
    let whole = [
        Vec2::ZERO,
        Vec2::new(look.image.width as f32, look.image.height as f32),
    ];
    let mut q = quad(p, Piece { to, from: whole }, look, clip)?;
    if turned {
        q.size = Vec2::new(q.size.y, q.size.x);
        q.rotation -= FRAC_PI_2;
    }
    Some(q)
}

/// A filled rectangle, `to` in the frame.
fn line(p: Placed, to: [Vec2; 2], color: [f32; 4], clip: [f32; 4]) -> GuiQuad {
    GuiQuad {
        center: p.screen_point((to[0] + to[1]) * 0.5),
        size: (to[1] - to[0]) * p.scale,
        rotation: p.rotation.to_radians(),
        color,
        clip,
        paint: Paint::Solid,
        shape: None,
        gradient: None,
    }
}
