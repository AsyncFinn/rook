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
    /// Message, and Hint, its subclass: drawn under the ScreenGuis.
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
    /// Properties neither layout nor drawing reads (what input and
    /// gamepad selection go by, which they read as they are).
    inert: Vec<PropertyId>,
    /// Properties that change only how a text object's glyphs are drawn
    /// (colors, transparencies), which the laid-out text carries: its text
    /// is laid out again alone, its size and place kept.
    text_looks: Vec<PropertyId>,
    /// Properties that place the changed object alone: where it goes and
    /// how big it is in its parent, turned, shown or clipping what it
    /// holds. Its siblings and
    /// parent don't depend on them unless the parent arranges, sizes or
    /// scrolls by its children.
    places: Vec<PropertyId>,
    /// Properties that change what an object holds in its own box (its
    /// text and image) or how a parent's UI layout orders it: they lay
    /// out again from the nearest object whose place doesn't depend on
    /// them, and draw again.
    contents: Vec<PropertyId>,
    /// `Instance.Parent`: a move journals [`Change::Reparented`] as well,
    /// which says what it left and joined.
    parent: Option<PropertyId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Touch {
    Nothing,
    /// How the changed object's tree looks.
    Look,
    /// How the changed text object's glyphs look.
    Text,
    /// Where the changed object and what it holds are.
    Place,
    /// What the changed object holds or shows, or a UI component's
    /// property: where things are from the nearest object whose place
    /// doesn't depend on it, and how it looks.
    Content,
    /// Where things are in the changed object's tree.
    Layout,
    /// GUI moved in the tree: the trees it left and joined lay out again.
    Tree,
    /// The screen the trees are laid out on.
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
                ("TextBox", "CursorPosition"),
                ("TextBox", "SelectionStart"),
                ("GuiObject", "Interactable"),
                ("GuiButton", "AutoButtonColor"),
                ("GuiButton", "Selected"),
                ("ImageButton", "HoverImageContent"),
                ("ImageButton", "PressedImageContent"),
                ("ScrollingFrame", "ScrollBarImageColor3"),
                ("ScrollingFrame", "ScrollBarImageTransparency"),
                ("ScrollingFrame", "TopImageContent"),
                ("ScrollingFrame", "MidImageContent"),
                ("ScrollingFrame", "BottomImageContent"),
                ("CanvasGroup", "GroupColor3"),
                ("CanvasGroup", "GroupTransparency"),
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
            .chain(["ImageLabel", "ImageButton"].into_iter().flat_map(|owner| {
                [
                    "ImageColor3",
                    "ImageTransparency",
                    "ImageRectOffset",
                    "ImageRectSize",
                    "ResampleMode",
                    "ScaleType",
                    "SliceCenter",
                    "SliceScale",
                    "TileSize",
                ]
                .map(|name| (owner, name))
            }))
            .filter_map(|(owner, name)| property(owner, name))
            .collect(),
            inert: [
                ("Instance", "Archivable"),
                ("GuiObject", "Active"),
                ("GuiObject", "Draggable"),
                ("GuiObject", "Selectable"),
                ("GuiObject", "SelectionImageObject"),
                ("GuiObject", "SelectionOrder"),
                ("GuiObject", "NextSelectionUp"),
                ("GuiObject", "NextSelectionDown"),
                ("GuiObject", "NextSelectionLeft"),
                ("GuiObject", "NextSelectionRight"),
                ("GuiObject", "SelectionGroup"),
                ("GuiButton", "Modal"),
            ]
            .into_iter()
            .filter_map(|(owner, name)| property(owner, name))
            .collect(),
            text_looks: ["TextLabel", "TextButton", "TextBox"]
                .into_iter()
                .flat_map(|owner| {
                    [
                        "TextColor3",
                        "TextTransparency",
                        "TextStrokeColor3",
                        "TextStrokeTransparency",
                    ]
                    .map(|name| property(owner, name))
                })
                .chain([property("TextBox", "PlaceholderColor3")])
                .flatten()
                .collect(),
            places: [
                "AnchorPoint",
                "ClipsDescendants",
                "Position",
                "Rotation",
                "Size",
                "Visible",
            ]
            .into_iter()
            .filter_map(|name| property("GuiObject", name))
            .collect(),
            contents: [
                ("Instance", "Name"),
                ("GuiObject", "LayoutOrder"),
                ("GuiObject", "AutomaticSize"),
                ("GuiObject", "SizeConstraint"),
                ("GuiObject", "BorderSizePixel"),
                ("GuiObject", "BorderMode"),
                ("Frame", "Style"),
                ("GuiButton", "Style"),
                ("ImageLabel", "ImageContent"),
                ("ImageButton", "ImageContent"),
                ("TextBox", "PlaceholderText"),
                ("ScrollingFrame", "CanvasSize"),
                ("ScrollingFrame", "CanvasPosition"),
                ("ScrollingFrame", "AutomaticCanvasSize"),
            ]
            .into_iter()
            .chain(
                ["TextLabel", "TextButton", "TextBox"]
                    .into_iter()
                    .flat_map(|owner| {
                        [
                            "Text",
                            "RichText",
                            "TextScaled",
                            "TextSize",
                            "TextWrapped",
                            "FontFace",
                            "TextXAlignment",
                            "TextYAlignment",
                            "LineHeight",
                            "MaxVisibleGraphemes",
                            "TextTruncate",
                        ]
                        .map(|name| (owner, name))
                    }),
            )
            .filter_map(|(owner, name)| property(owner, name))
            .collect(),
            parent: property("Instance", "Parent"),
        }
    }

    pub(crate) fn touches(&self, dom: &Dom, change: &Change) -> Touch {
        match change {
            Change::Property { property, .. } if self.inert.contains(property) => Touch::Nothing,
            Change::Property { property, .. } if self.looks.contains(property) => Touch::Look,
            Change::Property { property, .. } if self.text_looks.contains(property) => Touch::Text,
            Change::Property { property, .. }
                if [self.viewport, self.current_camera].contains(&Some(*property)) =>
            {
                Touch::All
            }
            Change::Property { property, .. } if self.places.contains(property) => Touch::Place,
            Change::Property { property, .. } if Some(*property) == self.parent => Touch::Nothing,
            Change::Property { id, .. } if !self.is_gui(dom, *id) => Touch::Nothing,
            Change::Property { property, .. } if self.contents.contains(property) => Touch::Content,
            Change::Property { id, .. } if self.is_component(dom, *id) => Touch::Content,
            Change::Property { .. } => Touch::Layout,
            // A Folder or a Player carrying GUI counts, so the whole
            // subtree is looked at.
            Change::Reparented { subtree, .. }
                if subtree.iter().any(|&id| self.is_gui(dom, id)) =>
            {
                Touch::Tree
            }
            _ => Touch::Nothing,
        }
    }

    /// A UI component (a layout, padding, UIScale, a size constraint):
    /// what it changes is its parent GUI object's.
    fn is_component(&self, dom: &Dom, id: InstanceId) -> bool {
        let (Some(inst), Some(ui)) = (dom.get(id), self.ui) else {
            return false;
        };
        dom.database().is_a(inst.class, ui)
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
