//! `GuiButton.Modal`: a shown Modal button frees a held mouse. The
//! buttons with Modal set are tracked from the journal as they change, so
//! asking whether one shows looks only at them, not every screen's GUI.

use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::PropertyId;
use rustc_hash::FxHashSet;

use crate::layout::{Layout, visible};

#[derive(Debug, Default)]
pub(crate) struct Modals {
    /// `GuiButton.Modal`, once resolved.
    modal: Option<PropertyId>,
    /// GuiButtons with Modal set, wherever they are.
    buttons: FxHashSet<InstanceId>,
}

impl Modals {
    /// Start tracking: every Modal button there already is.
    pub(crate) fn seed(&mut self, dom: &Dom) {
        self.modal = dom
            .database()
            .class_by_name("GuiButton")
            .and_then(|b| dom.database().find_property(b.id, "Modal"))
            .map(|p| p.id);
        for id in dom.descendants(dom.root()) {
            self.check(dom, id);
        }
    }

    /// Keep up with one journal change.
    pub(crate) fn note(&mut self, dom: &Dom, change: &Change) {
        match change {
            Change::Property { id, property } if Some(*property) == self.modal => {
                self.check(dom, *id);
            }
            // Created buttons are noted when they're parented.
            Change::Reparented { subtree, .. } => {
                for &id in subtree {
                    self.check(dom, id);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn forget(&mut self, id: InstanceId) {
        self.buttons.remove(&id);
    }

    fn check(&mut self, dom: &Dom, id: InstanceId) {
        let modal = !dom.is_destroyed(id)
            && dom.is_a(id, "GuiButton")
            && dom.bool_property(id, "Modal") == Some(true);
        if modal {
            self.buttons.insert(id);
        } else {
            self.buttons.remove(&id);
        }
    }

    /// Whether a Modal button shows on the local player's screen, as of
    /// the last layout: laid out, it and every GUI object above it
    /// Visible, in an Enabled ScreenGui in the PlayerGui or CoreGui
    /// (Folders between them count for nothing, as when drawing).
    pub(crate) fn shown(&self, dom: &Dom, layout: &Layout) -> bool {
        self.buttons
            .iter()
            .any(|&b| layout.placed.contains_key(&b) && on_screen(dom, b))
    }
}

fn on_screen(dom: &Dom, button: InstanceId) -> bool {
    let mut at = button;
    loop {
        if dom.is_a(at, "GuiObject") {
            if !visible(dom, at) {
                return false;
            }
        } else if dom.class_name(at) == "ScreenGui" {
            break;
        } else if dom.class_name(at) != "Folder" || at == button {
            return false;
        }
        let Some(parent) = dom.parent(at) else {
            return false;
        };
        at = parent;
    }
    if dom.bool_property(at, "Enabled") == Some(false) {
        return false;
    }
    let player_gui = dom
        .local_player()
        .and_then(|p| dom.find_first_child_of_class(p, "PlayerGui"));
    let core = dom.find_service("CoreGui");
    let mut holder = dom.parent(at);
    while let Some(h) = holder {
        if Some(h) == player_gui || Some(h) == core {
            return true;
        }
        if dom.class_name(h) != "Folder" {
            return false;
        }
        holder = dom.parent(h);
    }
    false
}
