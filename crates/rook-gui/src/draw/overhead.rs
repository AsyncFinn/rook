//! A name and health bar over a Humanoid's head, drawn as Studio draws
//! them (measured in screenshots of Studio 0.744:
//! docs/behavior/characters/overhead.md). Sizes are at the nearest size
//! step and scale with it, except the bar's height and end caps.

use std::sync::Arc;

use glam::Vec2;
use rook_types::{GuiQuad, Paint, WorldCanvas, font_from_enum};

use super::Text;
use crate::layout::Placed;
use crate::text::{self, Carets, Case, Source, Stroke, Style};
use crate::world::overhead::Look;

/// The bar's body (between its end caps) at full size, its height and
/// its caps' width.
const BODY: f32 = 96.0;
pub(crate) const BAR_HEIGHT: f32 = 8.0;
const CAP: f32 = 3.0;
/// Measured: SourceSans (its two-story g), at the size whose widths
/// match Studio's names to a pixel.
const TEXT_SIZE: f32 = 24.0;
const TRACK: f32 = 100.0 / 255.0;
/// Measured: the name's black outline, about a pixel wide, blends about
/// 60% at most (fitted to Studio's pixels around the letters).
const STROKE_OPACITY: f32 = 0.6;
const STROKE_WIDTH: f32 = 1.0;

/// The canvas's size in pixels: the bar's width, and the text over it.
pub(crate) fn size(look: &Look) -> Vec2 {
    Vec2::new(
        (BODY * look.scale).round() + 2.0 * CAP,
        text_box(look).y + BAR_HEIGHT,
    )
}

/// The box the name is laid out in, its bottom on the bar's top.
fn text_box(look: &Look) -> Vec2 {
    Vec2::new(
        (BODY * look.scale).round() + 2.0 * CAP,
        (TEXT_SIZE * look.scale * 1.25).ceil(),
    )
}

pub(crate) fn canvas(look: &Look, text: &mut Text<'_>) -> Arc<WorldCanvas> {
    let size = size(look);
    let mut out = Vec::new();
    let top = size.y - BAR_HEIGHT;
    // Observed: the bar draws over the name's descenders.
    if look.name_alpha > 0.0 {
        let box_size = text_box(look);
        name_quads(look, box_size, top, text, &mut out);
    }
    if let Some(bar) = &look.bar {
        bar_quads(size.x, top, bar.health, look.bar_alpha, &mut out);
    }
    Arc::new(WorldCanvas {
        quads: out,
        size,
        light_influence: 0.0,
        brightness: 1.0,
        // Measured: a far tag draws over a nearer head.
        always_on_top: true,
        max_distance: f32::INFINITY,
        // Measured: the bars' pixels are the health colors exactly, and
        // the stroke blends as screen GUI's does.
        untoned: true,
    })
}

fn name_quads(
    look: &Look,
    box_size: Vec2,
    bottom: f32,
    text: &mut Text<'_>,
    out: &mut Vec<GuiQuad>,
) {
    let source = Source {
        text: look.text.clone(),
        rich: false,
        style: Style {
            font: font_from_enum("SourceSans").unwrap_or_default(),
            size: TEXT_SIZE * look.scale,
            color: [1.0; 3],
            transparency: 1.0 - look.name_alpha,
            underline: false,
            strike: false,
            stroke: Some(Stroke {
                color: [0.0; 3],
                thickness: STROKE_WIDTH,
                transparency: 1.0 - STROKE_OPACITY * look.name_alpha,
            }),
            mark: None,
            case: Case::AsWritten,
        },
        wrapped: false,
        scaled: false,
        size_limits: None,
        align: Vec2::new(0.5, 1.0),
        line_height: 1.0,
        visible: None,
        carets: Carets::None,
    };
    let block = text::lay_out(text.fonts, &source, box_size, None);
    let placed = Placed {
        center: Vec2::new(box_size.x * 0.5, bottom - box_size.y * 0.5),
        local: box_size,
        scale: 1.0,
        rotation: 0.0,
    };
    super::text::emit(&block, (placed, GuiQuad::NO_CLIP), true, None, text, out);
}

/// The bar with its top at `top`, `width` wide: a translucent grey track
/// outlined in the health color, opaque grey end caps, and a two-pixel
/// strip of the color across `health` of the body.
fn bar_quads(width: f32, top: f32, health: f32, alpha: f32, out: &mut Vec<GuiQuad>) {
    let color = health_color(health);
    let paint = [color[0], color[1], color[2], alpha];
    let grey = |a: f32| [TRACK, TRACK, TRACK, a * alpha];
    let body = width - 2.0 * CAP;
    let mut rect = |x: f32, y: f32, w: f32, h: f32, c: [f32; 4]| {
        if w > 0.0 && h > 0.0 {
            out.push(GuiQuad {
                center: Vec2::new(x + w * 0.5, top + y + h * 0.5),
                size: Vec2::new(w, h),
                rotation: 0.0,
                color: c,
                clip: GuiQuad::NO_CLIP,
                paint: Paint::Solid,
                shape: None,
                gradient: None,
            });
        }
    };
    rect(CAP, 1.0, body, 6.0, grey(0.5));
    rect(1.0, 2.0, CAP - 1.0, 4.0, grey(1.0));
    rect(width - CAP, 2.0, CAP - 1.0, 4.0, grey(1.0));
    rect(CAP, 0.0, body, 1.0, paint);
    rect(CAP, 7.0, body, 1.0, paint);
    for x in [0.0, width - 1.0] {
        rect(x, 1.0, 1.0, 6.0, paint);
    }
    for x in [1.0, width - CAP] {
        rect(x, 1.0, CAP - 1.0, 1.0, paint);
        rect(x, 6.0, CAP - 1.0, 1.0, paint);
    }
    rect(
        CAP,
        3.0,
        (body * health.clamp(0.0, 1.0)).round(),
        2.0,
        paint,
    );
}

/// The bar's color at `health` (0 to 1): red to 10%, green from 80%, and
/// between, Shepard's interpolation of red at 10%, yellow at 50% and
/// green at 80% (Roblox's CoreScripts' `Healthbar3D`; measured: the
/// engine's 75% and 25% bars match it).
pub(crate) fn health_color(health: f32) -> [f32; 3] {
    const RED: [f32; 3] = [1.0, 28.0 / 255.0, 0.0];
    const YELLOW: [f32; 3] = [250.0 / 255.0, 235.0 / 255.0, 0.0];
    const GREEN: [f32; 3] = [27.0 / 255.0, 252.0 / 255.0, 107.0 / 255.0];
    const SAMPLES: [([f32; 3], f32); 3] = [(RED, 0.1), (YELLOW, 0.5), (GREEN, 0.8)];
    if health <= 0.1 {
        return RED;
    }
    if health >= 0.8 {
        return GREEN;
    }
    let mut sum = [0.0; 3];
    let mut weights = 0.0;
    for (color, at) in SAMPLES {
        let d = health - at;
        if d == 0.0 {
            return color;
        }
        let w = 1.0 / (d * d);
        for i in 0..3 {
            sum[i] += w * color[i];
        }
        weights += w;
    }
    sum.map(|c| c / weights)
}

#[cfg(test)]
mod tests {
    use super::health_color;

    fn rgb(health: f32) -> [u8; 3] {
        health_color(health).map(|c| (c * 255.0).round() as u8)
    }

    #[test]
    fn health_colors_match_studios_bars() {
        // Read from Studio 0.744's bars.
        assert_eq!(rgb(0.9), [27, 252, 107]);
        assert_eq!(rgb(0.5), [250, 235, 0]);
        let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1);
        assert!(near(rgb(0.75), [36, 251, 102]), "{:?}", rgb(0.75));
        assert!(near(rgb(0.25), [242, 91, 5]), "{:?}", rgb(0.25));
        assert_eq!(rgb(0.0), [255, 28, 0]);
    }
}
