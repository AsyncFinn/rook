//! Text in GUI objects: what TextLabels, TextButtons and TextBoxes show,
//! laid out as Roblox does (`TextBounds`, `TextFits`, wrapping,
//! `TextScaled`, `RichText`), as glyphs to draw. Rules measured in Studio
//! are in docs/behavior/gui/text.md.

mod fonts;
mod glyphs;
mod lines;
mod rich;
mod shape;

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::Font;

pub(crate) use fonts::{FaceId, Fonts};
pub(crate) use glyphs::Glyphs;
pub(crate) use rich::{Case, Stroke, Style};

use lines::{Item, Line, break_lines};

/// Measured: `TextScaled` text grows to at most a 100-pixel line (Legacy's
/// too, though its `TextSize` would then be 66.7).
const MAX_SCALED_PX: u32 = 100;

/// What some text is and how it's set, from a text object's properties
/// (or a Hint's or Message's).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Source {
    pub(crate) text: String,
    pub(crate) rich: bool,
    pub(crate) style: Style,
    pub(crate) wrapped: bool,
    pub(crate) scaled: bool,
    /// `TextXAlignment` and `TextYAlignment` as the share of spare room
    /// before the text.
    pub(crate) align: Vec2,
    pub(crate) line_height: f32,
    /// `MaxVisibleGraphemes`, when it limits anything.
    pub(crate) visible: Option<usize>,
}

impl Source {
    /// A TextLabel's, TextButton's or TextBox's text; `None` for other
    /// objects.
    pub(crate) fn of(dom: &Dom, id: InstanceId) -> Option<Self> {
        if !is_text(dom, id) {
            return None;
        }
        let color3 = |name, default: [f32; 3]| {
            dom.color3_property(id, name)
                .map_or(default, |c| [c.r, c.g, c.b])
        };
        let mut text = dom.string_property(id, "Text").unwrap_or_default();
        let mut color = color3("TextColor3", [0.0; 3]);
        if text.is_empty() && dom.class_name(id) == "TextBox" {
            // Inference: an empty TextBox shows its placeholder.
            text = dom
                .string_property(id, "PlaceholderText")
                .unwrap_or_default();
            color = color3("PlaceholderColor3", [0.7; 3]);
        }
        let stroke_transparency = dom
            .float_property(id, "TextStrokeTransparency")
            .unwrap_or(1.0);
        let style = Style {
            font: dom.font_property(id, "FontFace").unwrap_or_default(),
            size: dom.float_property(id, "TextSize").unwrap_or(14.0),
            color,
            transparency: dom.float_property(id, "TextTransparency").unwrap_or(0.0),
            underline: false,
            strike: false,
            stroke: (stroke_transparency < 1.0).then(|| Stroke {
                color: color3("TextStrokeColor3", [0.0; 3]),
                thickness: 1.0,
                transparency: stroke_transparency,
            }),
            mark: None,
            case: Case::AsWritten,
        };
        let share = |name, near: &str, far: &str| match dom.enum_item_property(id, name) {
            Some(n) if n == near => 0.0,
            Some(n) if n == far => 1.0,
            _ => 0.5,
        };
        let visible = dom
            .number_property(id, "MaxVisibleGraphemes")
            .filter(|&n| n >= 0.0)
            .map(|n| n as usize);
        Some(Self {
            text,
            rich: dom.bool_property(id, "RichText") == Some(true),
            style,
            wrapped: dom.bool_property(id, "TextWrapped") == Some(true),
            scaled: dom.bool_property(id, "TextScaled") == Some(true),
            align: Vec2::new(
                share("TextXAlignment", "Left", "Right"),
                share("TextYAlignment", "Top", "Bottom"),
            ),
            line_height: dom.float_property(id, "LineHeight").unwrap_or(1.0),
            visible,
        })
    }
}

/// Whether `id` shows text of its own.
pub(crate) fn is_text(dom: &Dom, id: InstanceId) -> bool {
    matches!(dom.class_name(id), "TextLabel" | "TextButton" | "TextBox")
}

/// A glyph placed in its object, in pixels from the object's top left
/// before UIScales.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlacedGlyph {
    pub(crate) face: FaceId,
    pub(crate) id: u32,
    /// The line height it's drawn at.
    pub(crate) px: f32,
    /// The pen on the baseline.
    pub(crate) pen: Vec2,
    pub(crate) color: [f32; 4],
    /// Its outline: color and thickness in pixels.
    pub(crate) stroke: Option<([f32; 4], f32)>,
}

/// A rectangle drawn with the text: a highlight behind it, or an
/// underline or strikethrough over it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TextRect {
    pub(crate) min: Vec2,
    pub(crate) max: Vec2,
    pub(crate) color: [f32; 4],
    pub(crate) behind: bool,
}

/// Text laid out in an object.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TextBlock {
    /// `TextBounds`.
    pub(crate) bounds: Vec2,
    /// `TextFits`.
    pub(crate) fits: bool,
    /// `ContentText`: the text without its markup.
    pub(crate) content: String,
    pub(crate) glyphs: Vec<PlacedGlyph>,
    pub(crate) rects: Vec<TextRect>,
}

impl TextBlock {
    /// Move everything drawn by `by`.
    pub(crate) fn shift(&mut self, by: Vec2) {
        self.glyphs.iter_mut().for_each(|g| g.pen += by);
        for r in &mut self.rects {
            r.min += by;
            r.max += by;
        }
    }
}

/// Runs shaped and broken into lines, before they're placed.
struct Set {
    lines: Vec<Line>,
    split: bool,
    /// Each line's top, and the whole height.
    tops: Vec<f32>,
    size: Vec2,
}

impl Set {
    /// Drop the lines below the area's bottom, keeping at least one.
    /// Measured: a word split over seven lines in a label five lines tall
    /// reports five lines' `TextBounds`. Whether any went.
    fn keep_fitting(&mut self, height: f32) -> bool {
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

/// Lay `source` out in a `size` area. `wrap` overrides where it wraps
/// (`None`: by `TextWrapped` at the area's width); AutomaticSize measures
/// with `Some(None)`, unwrapped.
pub(crate) fn lay_out(
    fonts: &mut Fonts,
    source: &Source,
    size: Vec2,
    wrap: Option<Option<f32>>,
) -> TextBlock {
    let spans = if source.rich {
        rich::parse(&source.text, &source.style)
            .unwrap_or_else(|| rich::plain(&source.text, &source.style))
    } else {
        rich::plain(&source.text, &source.style)
    };
    let content: String = spans.iter().map(|s| s.text.as_str()).collect();
    let wrap_at = wrap.unwrap_or((source.wrapped || source.scaled).then_some(size.x));
    let mut set = if source.scaled {
        scaled(fonts, source, &spans, size)
    } else {
        set_lines(fonts, source, &spans, None, wrap_at)
    };
    let mut fits = !set.split && set.size.x <= size.x && set.size.y <= size.y;
    if wrap.is_none() && wrap_at.is_some() {
        fits &= !set.keep_fitting(size.y);
    }
    let mut block = TextBlock {
        bounds: Vec2::new(set.size.x, set.size.y.floor()),
        fits,
        content,
        glyphs: Vec::new(),
        rects: Vec::new(),
    };
    place(&mut block, source, &spans, &set, size);
    block
}

/// The biggest whole-pixel line height (up to 100) whose text, wrapped,
/// fits the area without splitting a word (measured: "Hi" in a 200 × 50
/// Legacy label is 50 pixels tall; "Hello, world!" in 100 × 200 wraps to
/// two 41-pixel lines). Nothing fitting gives 1.
fn scaled(fonts: &mut Fonts, source: &Source, spans: &[rich::Span], size: Vec2) -> Set {
    let try_px =
        |fonts: &mut Fonts, px: u32| set_lines(fonts, source, spans, Some(px as f32), Some(size.x));
    let fits = |s: &Set| !s.split && s.size.x <= size.x && s.size.y <= size.y;
    let (mut lo, mut hi) = (1, MAX_SCALED_PX);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(&try_px(fonts, mid)) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    try_px(fonts, lo)
}

/// Shape `spans` (at line height `px` each, if given) and break them into
/// lines.
fn set_lines(
    fonts: &mut Fonts,
    source: &Source,
    spans: &[rich::Span],
    px: Option<f32>,
    wrap: Option<f32>,
) -> Set {
    let metrics = |fonts: &mut Fonts, font: &Font, size: f32| -> Option<(FaceId, f32, f32, f32)> {
        let picked = fonts.pick(font)?;
        let line_px = px.unwrap_or(size * picked.scale);
        let face = fonts.face(picked.face);
        let ascent = face.ascent / face.line * line_px;
        Some((picked.face, line_px, ascent, line_px - ascent))
    };
    let base = metrics(fonts, &source.style.font, source.style.size);
    let empty = base.map_or((0.0, 0.0), |(_, _, a, d)| (a, d));
    let mut items = Vec::new();
    for (run, span) in spans.iter().enumerate() {
        let Some((face, line_px, ascent, descent)) =
            metrics(fonts, &span.style.font, span.style.size)
        else {
            continue;
        };
        let text = match span.style.case {
            Case::AsWritten => span.text.clone(),
            // Approximation: small caps drawn as capitals.
            Case::Upper | Case::SmallCaps => span.text.to_uppercase(),
        };
        for (n, piece) in text.split('\n').enumerate() {
            if n > 0 {
                items.push(Item::Break { ascent, descent });
            }
            items.extend(
                shape::shape(fonts, face, line_px, piece, run)
                    .into_iter()
                    .map(Item::Glyph),
            );
        }
    }
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

/// Put `set`'s glyphs in a `size` area, aligned.
fn place(block: &mut TextBlock, source: &Source, spans: &[rich::Span], set: &Set, size: Vec2) {
    let top = ((size.y - set.size.y) * source.align.y).round();
    let mut shown = 0;
    for (line, &line_top) in set.lines.iter().zip(&set.tops) {
        let mut x = ((size.x - line.width) * source.align.x).round();
        let baseline = top + line_top + line.ascent;
        for g in &line.glyphs {
            let style = &spans[g.run].style;
            let alpha = 1.0 - style.transparency;
            let color = [style.color[0], style.color[1], style.color[2], alpha];
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
                    stroke: style.stroke.map(|s| {
                        // Inference: the outline fades with the text too.
                        let a = (1.0 - s.transparency) * alpha;
                        ([s.color[0], s.color[1], s.color[2], a], s.thickness)
                    }),
                });
            }
            shown += 1;
            x += g.advance;
        }
    }
}

#[cfg(test)]
mod tests;
