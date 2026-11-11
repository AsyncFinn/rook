//! Which object `GuiService:Select` picks (docs/behavior/gui/selection.md).

use rook_dom::{Dom, InstanceId};

use crate::Gui;
use crate::order::{self, Clip};

impl Gui {
    /// The object `GuiService:Select(parent)` selects, as laid out now: of
    /// `parent`'s shown descendants that are `Selectable`, interactable
    /// and on screen, the one whose top is highest, then leftmost, then
    /// drawn on top (measured; `Active` and transparency don't matter).
    pub fn first_selectable(&self, dom: &Dom, parent: InstanceId) -> Option<InstanceId> {
        let mut best: Option<(f32, f32, InstanceId)> = None;
        // Bottom first, so a later tie is drawn over the one kept.
        for screen in order::screens(dom) {
            if !(screen == parent || dom.is_ancestor_of(screen, parent)) {
                continue;
            }
            let Some(bounds) = self.layout.placed.get(&screen) else {
                continue;
            };
            let (screen_min, screen_max) = (bounds.top_left(), bounds.top_left() + bounds.size());
            for s in order::stack(dom, &self.layout, screen, Clip::NONE) {
                if !dom.is_ancestor_of(parent, s.id)
                    || dom.bool_property(s.id, "Selectable") != Some(true)
                    || dom.bool_property(s.id, "Interactable") == Some(false)
                {
                    continue;
                }
                // Its AbsolutePosition and AbsoluteSize.
                let (min, max) = (s.placed.top_left(), s.placed.top_left() + s.placed.size());
                if max.cmplt(screen_min).any() || min.cmpgt(screen_max).any() {
                    continue;
                }
                if best.is_none_or(|(y, x, _)| (min.y, min.x) <= (y, x)) {
                    best = Some((min.y, min.x, s.id));
                }
            }
        }
        best.map(|(_, _, id)| id)
    }
}
