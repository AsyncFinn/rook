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
    /// Properties that change only the drawing, which layout never
    /// reads: colors and transparencies scripts tween, the UI components
    /// drawn on a quad, and what input sets (`GuiState`, for
    /// AutoButtonColor's tints and a button's hover and pressed images; a
    /// TextBox's caret and selection).
    looks: Vec<PropertyId>,
    /// Properties that place the changed object alone: where it goes and
    /// how big it is in its parent, turned or shown. Its siblings and
    /// parent don't depend on them unless the parent arranges, sizes or
    /// scrolls by its children.
    places: Vec<PropertyId>,
}

/// What a change does to GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Touch {
    Nothing,
    /// How the changed object's tree looks.
    Look,
    /// Where the changed object and what it holds are.
    Place,
    /// Where things are in the changed object's tree.
    Layout,
    /// Which trees there are, or the screen they're laid out on.
    All,
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
                ("GuiObject", "BackgroundColor3"),
                ("GuiObject", "BackgroundTransparency"),
                ("GuiObject", "BorderColor3"),
                ("GuiObject", "GuiState"),
                ("TextLabel", "TextColor3"),
                ("TextButton", "TextColor3"),
                ("TextBox", "TextColor3"),
                ("TextLabel", "TextStrokeColor3"),
                ("TextButton", "TextStrokeColor3"),
                ("TextBox", "TextStrokeColor3"),
                ("TextBox", "CursorPosition"),
                ("TextBox", "SelectionStart"),
                ("ImageLabel", "ImageColor3"),
                ("ImageButton", "ImageColor3"),
                ("ImageLabel", "ImageTransparency"),
                ("ImageButton", "ImageTransparency"),
                ("UICorner", "CornerRadius"),
                ("UIGradient", "Color"),
                ("UIGradient", "Offset"),
                ("UIGradient", "Rotation"),
                ("UIGradient", "Transparency"),
                ("UIStroke", "Color"),
                ("UIStroke", "Transparency"),
                ("UIStroke", "Thickness"),
            ]
            .into_iter()
            .filter_map(|(owner, name)| property(owner, name))
            .collect(),
            places: ["AnchorPoint", "Position", "Rotation", "Size", "Visible"]
                .into_iter()
                .filter_map(|name| property("GuiObject", name))
                .collect(),
        }
    }

    pub(crate) fn touches(&self, dom: &Dom, change: &Change) -> Touch {
        match change {
            Change::Property { property, .. } if self.looks.contains(property) => Touch::Look,
            Change::Property { property, .. }
                if [self.viewport, self.current_camera].contains(&Some(*property)) =>
            {
                Touch::All
            }
            Change::Property { property, .. } if self.places.contains(property) => Touch::Place,
            Change::Property { id, .. } if self.is_gui(dom, *id) => Touch::Layout,
            // A Folder or a Player carrying GUI counts, so the whole
            // subtree is looked at.
            Change::Reparented { subtree, .. }
                if subtree.iter().any(|&id| self.is_gui(dom, id)) =>
            {
                Touch::All
            }
            _ => Touch::Nothing,
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
