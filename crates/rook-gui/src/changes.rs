//! Which journal entries can move or change GUI, and which only change
//! how it looks.

use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::{ClassId, PropertyId};

/// The classes and properties GUI depends on, resolved once.
#[derive(Debug)]
pub(crate) struct Watch {
    /// GuiBase2d: GUI objects and the LayerCollectors holding them.
    gui: Option<ClassId>,
    /// UIComponent: layouts, padding, UIScale and the size constraints.
    ui: Option<ClassId>,
    /// Message, and Hint, its subclass: drawn over the GUI.
    message: Option<ClassId>,
    /// `Camera.ViewportSize`, the screen's size, and
    /// `Workspace.CurrentCamera`, which camera's it is.
    viewport: Option<PropertyId>,
    current_camera: Option<PropertyId>,
    /// Properties input sets that change only the drawing: `GuiState`
    /// (AutoButtonColor's tints, a button's hover and pressed images),
    /// and a TextBox's caret and selection.
    looks: [Option<PropertyId>; 3],
}

/// What a change does to GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Touch {
    Nothing,
    Look,
    Layout,
}

impl Watch {
    pub(crate) fn new(dom: &Dom) -> Self {
        let db = dom.database();
        let class = |name| db.class_by_name(name).map(|c| c.id);
        let property = |owner, name| {
            class(owner)
                .and_then(|c| db.find_canonical_property(c, name))
                .map(|p| p.id)
        };
        Self {
            gui: class("GuiBase2d"),
            ui: class("UIComponent"),
            message: class("Message"),
            viewport: property("Camera", "ViewportSize"),
            current_camera: property("Workspace", "CurrentCamera"),
            looks: [
                property("GuiObject", "GuiState"),
                property("TextBox", "CursorPosition"),
                property("TextBox", "SelectionStart"),
            ],
        }
    }

    pub(crate) fn touches(&self, dom: &Dom, change: &Change) -> Touch {
        let layout = match change {
            Change::Property { property, .. } if self.looks.contains(&Some(*property)) => {
                return Touch::Look;
            }
            Change::Property { id, property } => {
                [self.viewport, self.current_camera].contains(&Some(*property))
                    || self.is_gui(dom, *id)
            }
            // A Folder or a Player carrying GUI counts, so the whole
            // subtree is looked at.
            Change::Reparented { subtree, .. } => subtree.iter().any(|&id| self.is_gui(dom, id)),
            _ => false,
        };
        if layout {
            Touch::Layout
        } else {
            Touch::Nothing
        }
    }

    fn is_gui(&self, dom: &Dom, id: InstanceId) -> bool {
        let Some(inst) = dom.get(id) else {
            return false;
        };
        let db = dom.database();
        [self.gui, self.ui, self.message]
            .into_iter()
            .flatten()
            .any(|c| db.is_a(inst.class, c))
    }
}
