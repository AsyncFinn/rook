//! Which journal entries can move or change GUI.

use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::{ClassId, PropertyId};

/// The classes and properties GUI depends on, resolved once.
#[derive(Debug)]
pub(crate) struct Watch {
    /// GuiBase2d: GUI objects and the LayerCollectors holding them.
    gui: Option<ClassId>,
    /// UIComponent: layouts, padding, UIScale and the size constraints.
    ui: Option<ClassId>,
    /// `Camera.ViewportSize`, the screen's size, and
    /// `Workspace.CurrentCamera`, which camera's it is.
    viewport: Option<PropertyId>,
    current_camera: Option<PropertyId>,
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
            viewport: property("Camera", "ViewportSize"),
            current_camera: property("Workspace", "CurrentCamera"),
        }
    }

    pub(crate) fn touches(&self, dom: &Dom, change: &Change) -> bool {
        match change {
            Change::Property { id, property } => {
                [self.viewport, self.current_camera].contains(&Some(*property))
                    || self.is_gui(dom, *id)
            }
            // A Folder or a Player carrying GUI counts, so the whole
            // subtree is looked at.
            Change::Reparented { subtree, .. } => subtree.iter().any(|&id| self.is_gui(dom, id)),
            _ => false,
        }
    }

    fn is_gui(&self, dom: &Dom, id: InstanceId) -> bool {
        let Some(inst) = dom.get(id) else {
            return false;
        };
        let db = dom.database();
        [self.gui, self.ui]
            .into_iter()
            .flatten()
            .any(|c| db.is_a(inst.class, c))
    }
}
