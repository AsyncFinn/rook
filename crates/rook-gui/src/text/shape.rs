//! Shaping runs of text into glyphs with Roblox's pixel advances
//! (docs/behavior/gui/text.md#widths).

use harfrust::{ShapeOptions, UnicodeBuffer};

use super::fonts::{FaceId, Fonts};

/// A shaped glyph, before it's put on a line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Glyph {
    pub(crate) face: FaceId,
    pub(crate) id: u32,
    /// The line height this glyph's text is set at, in pixels (`TextSize`,
    /// or half as big again for Legacy).
    pub(crate) px: f32,
    /// Whole pixels (measured: ten of a letter are always ten times one).
    pub(crate) advance: f32,
    /// The shaper's nudge from the pen, in pixels (y down).
    pub(crate) offset: [f32; 2],
    /// Above and below the baseline, in pixels.
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    /// Whitespace, where lines may wrap.
    pub(crate) space: bool,
    /// Which run it came from.
    pub(crate) run: usize,
}

/// Shape `text` (no line breaks) in `face` at `px`.
///
/// Measured: each glyph's advance, kerning included, is rounded up to a
/// whole pixel, with the face scaled so its ascent to descent is `px`.
/// About nine in ten glyphs match Studio exactly over sizes 8 to 100; the
/// rest are a pixel off, from hinting Rook doesn't copy (Approximation).
pub(crate) fn shape(fonts: &Fonts, face: FaceId, px: f32, text: &str, run: usize) -> Vec<Glyph> {
    if text.is_empty() {
        return Vec::new();
    }
    let f = fonts.face(face);
    let font = f.font();
    let shaper = f.shaper.shaper(&font).build();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let shaped = shaper.shape(buffer, ShapeOptions::new());
    let k = px / f.line;
    let ascent = f.ascent * k;
    let descent = px - ascent;
    shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .map(|(info, pos)| {
            let ch = text[info.cluster as usize..].chars().next().unwrap_or(' ');
            Glyph {
                face,
                id: info.glyph_id,
                px,
                // Less a hair, so float error can't push an exact width up.
                advance: (pos.x_advance as f32 * k - 1e-3).ceil().max(0.0),
                offset: [pos.x_offset as f32 * k, -(pos.y_offset as f32) * k],
                ascent,
                descent,
                space: ch.is_whitespace(),
                run,
            }
        })
        .collect()
}
