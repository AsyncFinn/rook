//! Text drawn as glyph quads over its object: highlights, outlines, the
//! glyphs, then underlines and strikethroughs.

use glam::Vec2;
use rook_types::GuiQuad;

use super::Text;
use crate::layout::Placed;
use crate::text::{FIELD_PX, PlacedGlyph, TextBlock};

/// Inference: an outline is the glyph drawn again under it, moved its
/// thickness each way and diagonally.
const OUTLINE: [Vec2; 8] = [
    Vec2::new(-1.0, 0.0),
    Vec2::new(1.0, 0.0),
    Vec2::new(0.0, -1.0),
    Vec2::new(0.0, 1.0),
    Vec2::new(-1.0, -1.0),
    Vec2::new(1.0, -1.0),
    Vec2::new(-1.0, 1.0),
    Vec2::new(1.0, 1.0),
];

/// Screen text this big or bigger, in pixels as drawn, is drawn from
/// distance fields; smaller text from masks at its size. Inference from
/// Roblox's flags (docs/behavior/gui/text.md#drawing): SDF fonts are on,
/// with a bitmap fallback threshold of 34.
const FIELDS_FROM: f32 = 34.0;

/// Glyphs are drawn from distance fields, which stay sharp at any size,
/// when `world` or [`FIELDS_FROM`] says so; otherwise from masks at the
/// size they're drawn.
pub(super) fn emit(
    block: &TextBlock,
    p: Placed,
    clip: [f32; 4],
    world: bool,
    text: &mut Text<'_>,
    out: &mut Vec<GuiQuad>,
) {
    let glyph = |g: &PlacedGlyph, nudge, color, text: &mut Text<'_>, out: &mut Vec<GuiQuad>| {
        let quad = if world || g.px * p.scale >= FIELDS_FROM {
            field_glyph(g, p, nudge, color, clip, text)
        } else {
            glyph(g, p, nudge, color, clip, text)
        };
        out.extend(quad);
    };
    let rect = |min: Vec2, max: Vec2, color: [f32; 4]| GuiQuad {
        center: p.screen_point((min + max) * 0.5),
        size: (max - min) * p.scale,
        rotation: p.rotation.to_radians(),
        color,
        clip,
        glyph: None,
    };
    for r in block.rects.iter().filter(|r| r.behind && r.color[3] > 0.0) {
        out.push(rect(r.min, r.max, r.color));
    }
    for g in &block.glyphs {
        if let Some((color, thickness)) = g.stroke.filter(|(c, _)| c[3] > 0.0) {
            for step in OUTLINE {
                glyph(g, step * thickness, color, text, out);
            }
        }
    }
    for g in block.glyphs.iter().filter(|g| g.color[3] > 0.0) {
        glyph(g, Vec2::ZERO, g.color, text, out);
    }
    for r in block.rects.iter().filter(|r| !r.behind && r.color[3] > 0.0) {
        out.push(rect(r.min, r.max, r.color));
    }
}

/// One glyph, its pen moved `nudge` screen pixels. Unturned text sits on
/// whole pixels, as Roblox's does (Inference: it looks crisp in Studio).
fn glyph(
    g: &PlacedGlyph,
    p: Placed,
    nudge: Vec2,
    color: [f32; 4],
    clip: [f32; 4],
    text: &mut Text<'_>,
) -> Option<GuiQuad> {
    let raster = text.glyphs.get(text.fonts, g.face, g.id, g.px * p.scale)?;
    let size = Vec2::new(raster.mask.width as f32, raster.mask.height as f32);
    let corner = Vec2::new(raster.left, raster.top);
    let center = if p.rotation == 0.0 {
        let pen = (p.screen_point(g.pen) + nudge).round();
        pen + corner + size * 0.5
    } else {
        let local = g.pen + (corner + size * 0.5 + nudge) / p.scale;
        p.screen_point(local)
    };
    Some(GuiQuad {
        center,
        size,
        rotation: p.rotation.to_radians(),
        color,
        clip,
        glyph: Some(raster.mask),
    })
}

/// One glyph from its distance field, stretched to its size, its pen moved
/// `nudge` pixels.
fn field_glyph(
    g: &PlacedGlyph,
    p: Placed,
    nudge: Vec2,
    color: [f32; 4],
    clip: [f32; 4],
    text: &mut Text<'_>,
) -> Option<GuiQuad> {
    let raster = text.glyphs.field(text.fonts, g.face, g.id)?;
    let k = g.px * p.scale / FIELD_PX;
    let size = Vec2::new(raster.mask.width as f32, raster.mask.height as f32) * k;
    let corner = Vec2::new(raster.left, raster.top) * k;
    let local = g.pen + (corner + size * 0.5 + nudge) / p.scale;
    Some(GuiQuad {
        center: p.screen_point(local),
        size,
        rotation: p.rotation.to_radians(),
        color,
        clip,
        glyph: Some(raster.mask),
    })
}
