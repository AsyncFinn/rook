//! Hint and Message (145 and 26 archive files): the old bars of text,
//! shown over all GUI while one is in Workspace or the local player,
//! later-made ones on top. Observed by eye: a Hint is an opaque black bar
//! across the window's top, over the top bar, shown even with no text; a
//! Message with no text shows nothing. The rest is Inference, as Studio's
//! captures leave GUI out (docs/behavior/gui/text.md#hint-and-message).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::{GuiQuad, Paint, font_from_enum};

use super::Text;
use crate::layout::{Placed, screen_size};
use crate::text::{self, Carets, Case, Source, Style};

/// A Hint's bar height, and both classes' text size (Legacy).
const HINT_HEIGHT: f32 = 20.0;
const HINT_TEXT: f32 = 12.0;
const MESSAGE_TEXT: f32 = 18.0;

pub(super) fn emit(dom: &Dom, text: &mut Text<'_>, out: &mut Vec<GuiQuad>) {
    let holders = dom
        .find_service("Workspace")
        .into_iter()
        .chain(dom.local_player());
    let mut shown: Vec<InstanceId> = holders
        .flat_map(|h| dom.children(h).iter().copied())
        .filter(|&c| match dom.class_name(c) {
            "Hint" => true,
            "Message" => !dom
                .string_property(c, "Text")
                .unwrap_or_default()
                .is_empty(),
            _ => false,
        })
        .collect();
    shown.sort_by_key(|&id| dom.creation_order(id));
    let screen = screen_size(dom);
    for id in shown {
        let hint = dom.class_name(id) == "Hint";
        // Both start at the window's top, ignoring the GUI inset.
        let (size, fill, text_size) = if hint {
            (
                Vec2::new(screen.x, HINT_HEIGHT),
                [0.0, 0.0, 0.0, 1.0],
                HINT_TEXT,
            )
        } else {
            (screen, [0.5, 0.5, 0.5, 0.5], MESSAGE_TEXT)
        };
        let placed = Placed {
            center: size * 0.5,
            local: size,
            scale: 1.0,
            rotation: 0.0,
        };
        out.push(GuiQuad {
            center: placed.center,
            size,
            rotation: 0.0,
            color: fill,
            clip: GuiQuad::NO_CLIP,
            paint: Paint::Solid,
            shape: None,
            gradient: None,
        });
        let source = Source {
            text: dom.string_property(id, "Text").unwrap_or_default(),
            rich: false,
            style: Style {
                font: font_from_enum("Legacy").unwrap_or_default(),
                size: text_size,
                color: [1.0; 3],
                transparency: 0.0,
                underline: false,
                strike: false,
                stroke: None,
                mark: None,
                case: Case::AsWritten,
            },
            wrapped: !hint,
            scaled: false,
            size_limits: None,
            align: Vec2::splat(0.5),
            line_height: 1.0,
            visible: None,
            carets: Carets::None,
        };
        let block = text::lay_out(text.fonts, &source, size, None);
        super::text::emit(&block, (placed, GuiQuad::NO_CLIP), false, None, text, out);
    }
}
