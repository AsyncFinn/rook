//! A legacy style's texture, in place of the object's background and
//! border (docs/behavior/gui/styles.md#drawing).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::GuiQuad;

use super::image::{Look, Piece, quad};
use crate::images::Images;
use crate::layout::Placed;
use crate::style::Style;

/// Measured: the texture alone, untinted and opaque whatever the
/// object's `BackgroundColor3`, `BackgroundTransparency`, border and
/// `AutoButtonColor`, within the object's box.
pub(super) fn emit(
    dom: &Dom,
    id: InstanceId,
    style: Style,
    p: Placed,
    clip: [f32; 4],
    images: &mut Images,
    out: &mut Vec<GuiQuad>,
) {
    // Both of a button's textures are asked for at once, so the first
    // hover doesn't blink while the lit one loads.
    for url in [style.idle, style.lit, style.chosen] {
        images.get(url);
    }
    let Some(image) = images.get(style.shown(dom, id)) else {
        return;
    };
    let look = Look {
        image,
        color: [1.0; 4],
        pixelated: false,
        tiled: false,
    };
    let texels = Vec2::new(image.width as f32, image.height as f32);
    out.extend(
        pieces(p.local, texels, style.corner)
            .into_iter()
            .filter_map(|piece| quad(p, piece, look, clip)),
    );
}

/// Nine pieces: each corner `corner` texture pixels square, drawn at that
/// size until the object's shorter side is under two corners, then shrunk
/// with it; the edges and middle stretched between. Measured: a corner can
/// reach past the texture's middle (a 24-pixel button texture has 18-pixel
/// corners), the middle then drawn from between them, reversed.
fn pieces(size: Vec2, texels: Vec2, corner: f32) -> Vec<Piece> {
    let size = size.abs();
    let fit = (size.min_element() / (2.0 * corner)).min(1.0);
    let near = Vec2::splat(corner * fit);
    let to = [Vec2::ZERO, near, size - near, size];
    let k = Vec2::splat(corner);
    let from = [Vec2::ZERO, k, texels - k, texels];
    let mut out = Vec::with_capacity(9);
    for row in 0..3 {
        for col in 0..3 {
            let pick =
                |points: &[Vec2; 4], i: usize| Vec2::new(points[i + col].x, points[i + row].y);
            out.push(Piece {
                to: [pick(&to, 0), pick(&to, 1)],
                from: [pick(&from, 0), pick(&from, 1)],
            });
        }
    }
    out
}
