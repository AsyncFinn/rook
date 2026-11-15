//! Setting text in a box: shaping each line, wrapping and scaling it to fit,
//! and placing its glyphs and decorations.

use super::*;

impl TextBlock {
    pub(crate) fn shift(&mut self, by: Vec2) {
        self.glyphs.iter_mut().for_each(|g| g.pen += by);
        for r in &mut self.rects {
            r.min += by;
            r.max += by;
        }
        for line in &mut self.carets {
            line.top += by.y;
            line.bottom += by.y;
            line.stops.iter_mut().for_each(|s| s.1 += by.x);
        }
    }

    /// Where the caret before character `index` stands: its top, and the
    /// line's height. Where a line wrapped, the start of the next line.
    pub(crate) fn caret(&self, index: usize) -> Option<(Vec2, f32)> {
        let found = self.carets.iter().rev().find_map(|line| {
            let &(_, x) = line.stops.iter().find(|s| s.0 == index)?;
            Some((line, x))
        });
        let (line, x) = match found {
            Some(f) => f,
            None => {
                let line = self.carets.last()?;
                (line, line.stops.last()?.1)
            }
        };
        Some((Vec2::new(x, line.top), line.bottom - line.top))
    }

    /// The character a caret put at `at` stands before: on the nearest
    /// line, at the nearest place.
    pub(crate) fn index_at(&self, at: Vec2) -> Option<usize> {
        let line = self.carets.iter().min_by(|a, b| {
            let off = |l: &CaretLine| (l.top - at.y).max(at.y - l.bottom).max(0.0);
            off(a).total_cmp(&off(b))
        })?;
        let stop = line
            .stops
            .iter()
            .min_by(|a, b| (a.1 - at.x).abs().total_cmp(&(b.1 - at.x).abs()))?;
        Some(stop.0)
    }
}

/// Runs shaped and broken into lines, before they're placed.
pub(super) struct Set {
    pub(super) lines: Vec<Line>,
    pub(super) split: bool,
    /// Each line's top, and the whole height.
    pub(super) tops: Vec<f32>,
    pub(super) size: Vec2,
}

impl Set {
    /// Drop the lines below the area's bottom, keeping at least one.
    /// Measured: a word split over seven lines in a label five lines tall
    /// reports five lines' `TextBounds`. Whether any went.
    pub(super) fn keep_fitting(&mut self, height: f32) -> bool {
        let keep = self
            .lines
            .iter()
            .zip(&self.tops)
            .take_while(|(l, top)| *top + l.ascent + l.descent <= height)
            .count()
            .max(1);
        if keep == self.lines.len() {
            return false;
        }
        self.lines.truncate(keep);
        self.tops.truncate(keep);
        let last = &self.lines[keep - 1];
        self.size = Vec2::new(
            self.lines.iter().map(|l| l.width).fold(0.0, f32::max),
            self.tops[keep - 1] + last.ascent + last.descent,
        );
        true
    }
}

/// The biggest whole-pixel line height (up to 100) whose text, wrapped,
/// fits the area without splitting a word (measured: "Hi" in a 200 × 50
/// Legacy label is 50 pixels tall; "Hello, world!" in 100 × 200 wraps to
/// two 41-pixel lines). Nothing fitting gives 1. A UITextSizeConstraint
/// holds it between its sizes, the least even when that doesn't fit.
pub(super) fn scaled(
    fonts: &Fonts,
    source: &Source,
    spans: &[rich::Span],
    shaped: &Shaped,
    size: Vec2,
) -> Set {
    let try_px = |px: u32| set_lines(fonts, source, spans, shaped, Some(px as f32), Some(size.x));
    let fits = |s: &Set| !s.split && s.size.x <= size.x && s.size.y <= size.y;
    let (mut lo, mut hi) = (1, MAX_SCALED_PX);
    if let Some((min, max)) = source.size_limits {
        // The limits are `TextSize`s; lines are that times the font's scale.
        let scale = shaped.base.map_or(1.0, |p| p.scale);
        let px = |s: f32| ((s * scale).round() as u32).clamp(1, MAX_SCALED_PX);
        lo = px(min);
        hi = px(max).max(lo);
    }
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(&try_px(mid)) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    try_px(lo)
}

/// Text shaped, before it's set at a size. Shaping works in font units, so
/// TextScaled shapes once and sets that at each size it tries. Line breaks
/// can't be found once for every size: advances round up to whole pixels
/// at each size, so widths don't grow in proportion and a word that fits
/// at one size can wrap at a bigger one. So each size breaks lines again,
/// which is cheap next to shaping.
pub(super) struct Shaped {
    /// The text's own face and scale, which sets an empty line's height.
    pub(super) base: Option<Picked>,
    /// Each span's, `None` when its font isn't loaded.
    pub(super) runs: Vec<Option<Picked>>,
    /// Glyphs and newlines, each with the span it's in.
    pub(super) items: Vec<(usize, Shape)>,
}

pub(super) enum Shape {
    Glyph(shape::Shaped),
    /// A newline, character `index` of the text.
    Break(usize),
}

impl Shaped {
    pub(super) fn new(fonts: &mut Fonts, source: &Source, spans: &[rich::Span]) -> Self {
        let base = fonts.pick(&source.style.font);
        let mut runs = Vec::with_capacity(spans.len());
        let mut items = Vec::new();
        // The characters before each span, so glyphs know where they are in
        // the whole text.
        let mut at = 0;
        for (run, span) in spans.iter().enumerate() {
            let first = at;
            at += span.text.chars().count();
            let picked = fonts.pick(&span.style.font);
            runs.push(picked);
            let Some(picked) = picked else {
                continue;
            };
            let text = match span.style.case {
                Case::AsWritten => span.text.clone(),
                // Approximation: small caps drawn as capitals.
                Case::Upper | Case::SmallCaps => span.text.to_uppercase(),
            };
            let mut index = first;
            for (n, piece) in text.split('\n').enumerate() {
                if n > 0 {
                    items.push((run, Shape::Break(index)));
                    index += 1;
                }
                items.extend(
                    fonts
                        .shape(picked.face, piece, index)
                        .into_iter()
                        .map(|g| (run, Shape::Glyph(g))),
                );
                index += piece.chars().count();
            }
        }
        Self { base, runs, items }
    }
}

/// Set `shaped` (at line height `px` each, if given) and break it into
/// lines.
pub(super) fn set_lines(
    fonts: &Fonts,
    source: &Source,
    spans: &[rich::Span],
    shaped: &Shaped,
    px: Option<f32>,
    wrap: Option<f32>,
) -> Set {
    let metrics = |picked: Picked, size: f32| -> (shape::Scale, f32, f32) {
        let line_px = px.unwrap_or(size * picked.scale);
        let face = fonts.face(picked.face);
        let ascent = face.ascent / face.line * line_px;
        let scale = shape::Scale::new(face, picked.face, line_px);
        (scale, ascent, line_px - ascent)
    };
    let empty = shaped.base.map_or((0.0, 0.0), |p| {
        let (_, a, d) = metrics(p, source.style.size);
        (a, d)
    });
    let runs: Vec<_> = (shaped.runs.iter().zip(spans))
        .map(|(p, span)| p.map(|p| metrics(p, span.style.size)))
        .collect();
    let items: Vec<Item> = (shaped.items.iter())
        .map(|(run, item)| {
            let run = *run;
            let (scale, ascent, descent) = runs[run].expect("only loaded runs are shaped");
            match *item {
                Shape::Glyph(g) => Item::Glyph(g.at(&scale, run)),
                Shape::Break(index) => Item::Break {
                    ascent,
                    descent,
                    index,
                },
            }
        })
        .collect();
    let (lines, split) = break_lines(&items, wrap, empty);
    // Measured: n lines of height h are h + (n - 1) × h × LineHeight tall.
    let mut tops = Vec::with_capacity(lines.len());
    let mut y = 0.0;
    let mut bottom: f32 = 0.0;
    for line in &lines {
        tops.push(y);
        let h = line.ascent + line.descent;
        bottom = y + h;
        y += h * source.line_height;
    }
    let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    Set {
        lines,
        split,
        tops,
        size: Vec2::new(width, bottom),
    }
}

/// A glyph's color in `style`, and its outline's color and thickness.
pub(super) fn paint(style: &Style) -> ([f32; 4], Option<([f32; 4], f32)>) {
    let alpha = 1.0 - style.transparency;
    let color = [style.color[0], style.color[1], style.color[2], alpha];
    let stroke = style.stroke.map(|s| {
        // Inference: the outline fades with the text too.
        let a = (1.0 - s.transparency) * alpha;
        ([s.color[0], s.color[1], s.color[2], a], s.thickness)
    });
    (color, stroke)
}

impl TextBlock {
    /// Give every glyph `style`'s colors: plain text's glyphs all take
    /// its one style (see [`Source::recolors`]).
    pub(crate) fn recolor(&mut self, style: &Style) {
        let (color, stroke) = paint(style);
        for g in &mut self.glyphs {
            g.color = color;
            g.stroke = stroke;
        }
    }
}

pub(super) fn place(
    block: &mut TextBlock,
    source: &Source,
    spans: &[rich::Span],
    set: &Set,
    size: Vec2,
) {
    let top = ((size.y - set.size.y) * source.align.y).round();
    let mut shown = 0;
    for (line, &line_top) in set.lines.iter().zip(&set.tops) {
        let mut x = ((size.x - line.width) * source.align.x).round();
        let baseline = top + line_top + line.ascent;
        let mut carets = CaretLine {
            top: top + line_top,
            bottom: baseline + line.descent,
            stops: vec![(line.start(), x)],
        };
        for g in &line.glyphs {
            let style = &spans[g.run].style;
            let (color, stroke) = paint(style);
            let left = Vec2::new(x, baseline - g.ascent);
            let right = Vec2::new(x + g.advance, baseline + g.descent);
            if let Some((c, t)) = style.mark {
                block.rects.push(TextRect {
                    min: left,
                    max: right,
                    color: [c[0], c[1], c[2], 1.0 - t],
                    behind: true,
                });
            }
            // Inference: lines a sixteenth of the size thick, at most a
            // pixel thin; underline just below the baseline, strike
            // through the middle of lowercase.
            let thick = (g.px / 16.0).max(1.0);
            for (on, y) in [
                (style.underline, baseline + thick),
                (style.strike, baseline - g.ascent * 0.35),
            ] {
                if on {
                    block.rects.push(TextRect {
                        min: Vec2::new(left.x, y),
                        max: Vec2::new(right.x, y + thick),
                        color,
                        behind: false,
                    });
                }
            }
            if source.visible.is_none_or(|n| shown < n) && !g.space {
                block.glyphs.push(PlacedGlyph {
                    face: g.face,
                    id: g.id,
                    px: g.px,
                    pen: Vec2::new(x + g.offset[0], baseline + g.offset[1]),
                    color,
                    stroke,
                });
            }
            shown += 1;
            x += g.advance;
            carets.stops.push((g.index + 1, x));
        }
        match source.carets {
            Carets::Text => block.carets.push(carets),
            Carets::Placeholder if block.carets.is_empty() => {
                carets.stops.truncate(1);
                carets.stops[0].0 = 0;
                block.carets.push(carets);
            }
            _ => {}
        }
    }
}
