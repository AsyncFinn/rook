//! Shaping runs of text into glyphs with Roblox's pixel advances
//! (docs/behavior/gui/text.md#widths).

use std::sync::Arc;

use harfrust::{ShapeOptions, UnicodeBuffer};
use rustc_hash::FxHashMap;

use super::fonts::{Face, FaceId};

/// How many runs each generation of [`Runs`] keeps.
const RUNS_KEPT: usize = 4096;

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
    pub(crate) run: usize,
    pub(crate) index: usize,
}

/// A glyph as the shaper placed it, in font units. Shaping doesn't depend
/// on the size text is set at, so one shaping serves every size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Shaped {
    id: u32,
    advance: i32,
    offset: [i32; 2],
    space: bool,
    index: usize,
}

/// A face set at a line height: what turns font units into pixels.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Scale {
    face: FaceId,
    px: f32,
    k: f32,
    ascent: f32,
    descent: f32,
}

impl Scale {
    /// Measured: the face is scaled so its ascent to descent is `px`.
    pub(crate) fn new(f: &Face, face: FaceId, px: f32) -> Self {
        let k = px / f.line;
        let ascent = f.ascent * k;
        Self {
            face,
            px,
            k,
            ascent,
            descent: px - ascent,
        }
    }
}

impl Shaped {
    /// The glyph set at `scale`, as part of run `run`.
    ///
    /// Measured: each glyph's advance, kerning included, is rounded up to
    /// a whole pixel. About nine in ten glyphs match Studio exactly over
    /// sizes 8 to 100; the rest are a pixel off, from hinting Rook doesn't
    /// copy (Approximation).
    pub(crate) fn at(&self, scale: &Scale, run: usize) -> Glyph {
        let k = scale.k;
        Glyph {
            face: scale.face,
            id: self.id,
            px: scale.px,
            // Less a hair, so float error can't push an exact width up.
            advance: (self.advance as f32 * k - 1e-3).ceil().max(0.0),
            offset: [self.offset[0] as f32 * k, -(self.offset[1] as f32) * k],
            ascent: scale.ascent,
            descent: scale.descent,
            space: self.space,
            run,
            index: self.index,
        }
    }
}

#[cfg(test)]
thread_local! {
    /// How many times text was shaped on this thread, for tests that bound it.
    pub(crate) static SHAPINGS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Runs of text shaped before, by face: shaping is the costly part of
/// laying text out, and doesn't depend on the size text is set at, so a
/// label set to text it showed before (a list's rows as it scrolls, a
/// counter's digits) isn't shaped again, as browsers keep a word cache.
/// Runs used since the last generation turned stay; the generation turns
/// when it holds [`RUNS_KEPT`], dropping what it didn't use.
#[derive(Debug, Default)]
pub(crate) struct Runs {
    fresh: FxHashMap<FaceId, FxHashMap<Box<str>, Arc<[Shaped]>>>,
    old: FxHashMap<FaceId, FxHashMap<Box<str>, Arc<[Shaped]>>>,
    kept: usize,
}

impl Runs {
    /// `text` (no line breaks) shaped in `face`, its first character
    /// character `first` of the whole text.
    pub(crate) fn shape(
        &mut self,
        f: &Face,
        face: FaceId,
        text: &str,
        first: usize,
    ) -> Vec<Shaped> {
        if text.is_empty() {
            return Vec::new();
        }
        let shaped = match self.fresh.get(&face).and_then(|runs| runs.get(text)) {
            Some(shaped) => shaped.clone(),
            None => {
                let kept = self.old.get_mut(&face).and_then(|runs| runs.remove(text));
                let shaped = kept.unwrap_or_else(|| shape(f, text).into());
                if self.kept >= RUNS_KEPT {
                    self.old = std::mem::take(&mut self.fresh);
                    self.kept = 0;
                }
                let runs = self.fresh.entry(face).or_default();
                runs.insert(text.into(), shaped.clone());
                self.kept += 1;
                shaped
            }
        };
        (shaped.iter())
            .map(|g| Shaped {
                index: g.index + first,
                ..*g
            })
            .collect()
    }
}

/// Shape `text` (no line breaks) in face `f`, its characters counted from
/// its start.
fn shape(f: &Face, text: &str) -> Vec<Shaped> {
    #[cfg(test)]
    SHAPINGS.with(|n| n.set(n.get() + 1));
    let font = f.font();
    let shaper = f.shaper.shaper(&font).build();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let shaped = shaper.shape(buffer, ShapeOptions::new());
    shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .map(|(info, pos)| {
            let cluster = info.cluster as usize;
            let ch = text[cluster..].chars().next().unwrap_or(' ');
            Shaped {
                id: info.glyph_id,
                advance: pos.x_advance,
                offset: [pos.x_offset, pos.y_offset],
                space: ch.is_whitespace(),
                index: text[..cluster].chars().count(),
            }
        })
        .collect()
}
