//! Hint and Message (145 and 26 archive files): the old bars of text,
//! shown under all ScreenGuis while one is in Workspace or the local
//! player, later-made ones on top. Measured from window captures of Studio
//! (docs/behavior/gui/text.md#hint-and-message).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::{GuiQuad, Paint, font_from_enum};
use rustc_hash::FxHashSet;

use super::Text;
use crate::layout::{Placed, screen_size};
use crate::text::{self, Carets, Case, Source, Stroke, Style};

/// A Hint's bar height. Measured.
const HINT_HEIGHT: f32 = 20.0;
/// Measured: both draw their text as a TextLabel with Arial at
/// `TextSize` 21 would, centered on the bar or window (pixel for pixel).
const TEXT_SIZE: f32 = 21.0;

pub(crate) fn is_message(dom: &Dom, id: InstanceId) -> bool {
    dom.is_a(id, "Message")
}

/// Draw those of `candidates` (Hints and Messages that have been in the
/// Workspace or a player) that show.
pub(super) fn emit(
    dom: &Dom,
    candidates: &FxHashSet<InstanceId>,
    text: &mut Text<'_>,
    out: &mut Vec<GuiQuad>,
) {
    let holders = [dom.find_service("Workspace"), dom.local_player()];
    let mut shown: Vec<InstanceId> = (candidates.iter().copied())
        .filter(|&c| dom.parent(c).is_some_and(|p| holders.contains(&Some(p))))
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
        let (size, fill) = if hint {
            (Vec2::new(screen.x, HINT_HEIGHT), [0.0, 0.0, 0.0, 1.0])
        } else {
            (screen, [0.5, 0.5, 0.5, 0.5])
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
                font: font_from_enum("Arial").unwrap_or_default(),
                size: TEXT_SIZE,
                color: [1.0; 3],
                transparency: 0.0,
                underline: false,
                strike: false,
                // A Message's text has a black TextStroke; a Hint's has
                // none (its descenders reach below the bar without one).
                stroke: (!hint).then_some(Stroke {
                    color: [0.0; 3],
                    thickness: 1.0,
                    transparency: 0.0,
                }),
                mark: None,
                case: Case::AsWritten,
            },
            // Neither wraps: a long line runs off the window's sides.
            wrapped: false,
            scaled: false,
            size_limits: None,
            align: Vec2::splat(0.5),
            line_height: 1.0,
            visible: None,
            carets: Carets::None,
        };
        let block = text::lay_out(text.fonts, &source, size, None);
        // Nor is the text cut to the bar.
        super::text::emit(&block, (placed, GuiQuad::NO_CLIP), false, None, text, out);
    }
}
