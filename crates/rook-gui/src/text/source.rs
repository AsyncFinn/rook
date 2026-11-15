//! What text an object shows and how: its string, font, size, wrapping,
//! alignment and rich text, read off its properties.

use super::*;

impl Source {
    /// Whether text laid out from `self` lays out as `other` does but for
    /// its glyphs' colors: plain text whose color, transparency or stroke
    /// alone changed, which [`TextBlock::recolor`] gives it.
    pub(crate) fn recolors(&self, other: &Source) -> bool {
        if self.rich || other.rich {
            return false;
        }
        let style = Style {
            color: other.style.color,
            transparency: other.style.transparency,
            stroke: other.style.stroke,
            ..self.style.clone()
        };
        Source {
            style,
            ..self.clone()
        } == *other
    }

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
        let is_box = dom.class_name(id) == "TextBox";
        let mut carets = if is_box { Carets::Text } else { Carets::None };
        if text.is_empty() && is_box {
            carets = Carets::Placeholder;
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
            size_limits: dom
                .find_first_child_of_class(id, "UITextSizeConstraint")
                .map(|c| {
                    let limit =
                        |name, default: f64| dom.number_property(c, name).unwrap_or(default);
                    (
                        limit("MinTextSize", 1.0) as f32,
                        limit("MaxTextSize", 100.0) as f32,
                    )
                }),
            align: Vec2::new(
                share("TextXAlignment", "Left", "Right"),
                share("TextYAlignment", "Top", "Bottom"),
            ),
            line_height: dom.float_property(id, "LineHeight").unwrap_or(1.0),
            visible,
            carets,
        })
    }

    /// Plain, left-aligned text in one font, as `TextService` measures it.
    pub(crate) fn plain(text: &str, font: Font, size: f32) -> Self {
        Self {
            text: text.to_owned(),
            rich: false,
            style: Style {
                font,
                size,
                color: [0.0; 3],
                transparency: 0.0,
                underline: false,
                strike: false,
                stroke: None,
                mark: None,
                case: Case::AsWritten,
            },
            wrapped: true,
            scaled: false,
            size_limits: None,
            align: Vec2::ZERO,
            line_height: 1.0,
            visible: None,
            carets: Carets::None,
        }
    }
}
