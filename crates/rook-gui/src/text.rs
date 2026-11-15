//! Text in GUI objects: what TextLabels, TextButtons and TextBoxes show,
//! laid out as Roblox does (`TextBounds`, `TextFits`, wrapping,
//! `TextScaled`, `RichText`), as glyphs to draw. Rules measured in Studio
//! are in docs/behavior/gui/text.md.

mod distance;
mod fonts;
mod glyphs;
mod lines;
mod placing;
mod rich;
mod shape;
mod source;

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::Font;

pub(crate) use distance::FIELD_PX;
use fonts::Picked;
pub(crate) use fonts::{FaceId, Fonts};
pub(crate) use glyphs::Glyphs;
use placing::*;
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
    /// A UITextSizeConstraint's `MinTextSize` and `MaxTextSize`, which
    /// hold scaled text's `TextSize` (creator-docs UITextSizeConstraint).
    pub(crate) size_limits: Option<(f32, f32)>,
    /// `TextXAlignment` and `TextYAlignment` as the share of spare room
    /// before the text.
    pub(crate) align: Vec2,
    pub(crate) line_height: f32,
    /// `MaxVisibleGraphemes`, when it limits anything.
    pub(crate) visible: Option<usize>,
    pub(crate) carets: Carets,
}

/// Whether laid-out text keeps where a caret can stand: a TextBox's does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Carets {
    None,
    Text,
    /// It shows the placeholder, so the caret can only be at its start.
    Placeholder,
}

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
    /// Where a caret can stand, line by line, for a TextBox.
    pub(crate) carets: Vec<CaretLine>,
}

/// The places a caret can stand on one line, in pixels from the object's
/// top left before UIScales.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CaretLine {
    pub(crate) top: f32,
    pub(crate) bottom: f32,
    /// Each place: the character after it, and its x.
    pub(crate) stops: Vec<(usize, f32)>,
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
    let shaped = Shaped::new(fonts, source, &spans);
    let mut set = if source.scaled {
        scaled(fonts, source, &spans, &shaped, size)
    } else {
        set_lines(fonts, source, &spans, &shaped, None, wrap_at)
    };
    // Measured: unwrapped text taller than the object still fits (21-pixel
    // text in a 120 × 10 button).
    let tall = wrap_at.is_some() && set.size.y > size.y;
    let mut fits = !set.split && set.size.x <= size.x && !tall;
    if wrap.is_none() && wrap_at.is_some() {
        fits &= !set.keep_fitting(size.y);
    }
    let mut block = TextBlock {
        bounds: Vec2::new(set.size.x, set.size.y.floor()),
        fits,
        content,
        glyphs: Vec::new(),
        rects: Vec::new(),
        carets: Vec::new(),
    };
    place(&mut block, source, &spans, &set, size);
    block
}

#[cfg(test)]
mod tests;
