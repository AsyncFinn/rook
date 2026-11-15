//! The cursor shown: the mouse's icon, a hovered GUI button's or
//! ClickDetector's, or the system cursor Roblox's own icons stand for.

use super::*;

pub(super) const ARROW: &str = "rbxasset://textures/Cursors/KeyboardMouse/ArrowFarCursor.png";

pub(super) const HAND: &str = "rbxasset://textures/Cursors/KeyboardMouse/ArrowCursor.png";

pub(super) const IBEAM: &str = "rbxasset://textures/Cursors/KeyboardMouse/IBeamCursor.png";

impl CursorLook {
    /// What shows while this look's image loads, or when it can't: the
    /// system's nearest for Roblox's own (no install), the arrow for a
    /// script's icon.
    pub fn stand_in(&self) -> Option<CursorLook> {
        let CursorLook::Image(url) = self else {
            return None;
        };
        Some(match url.as_str() {
            ARROW => CursorLook::System(SystemCursor::Arrow),
            HAND => CursorLook::System(SystemCursor::PointingHand),
            IBEAM => CursorLook::System(SystemCursor::IBeam),
            _ => CursorLook::Image(ARROW.to_owned()),
        })
    }

    pub(super) fn icon(url: String) -> CursorLook {
        let system = match url.strip_prefix("rbxasset://SystemCursors/") {
            Some("Arrow") => SystemCursor::Arrow,
            Some("PointingHand") => SystemCursor::PointingHand,
            Some("IBeam") => SystemCursor::IBeam,
            Some("OpenHand") => SystemCursor::OpenHand,
            Some("ClosedHand") => SystemCursor::ClosedHand,
            _ => return CursorLook::Image(url),
        };
        CursorLook::System(system)
    }
}

impl DataModel {
    /// What the cursor shows: none with `UserInputService.MouseIconEnabled`
    /// off, else `UserInputService.MouseIcon`, else a held Tool's Mouse's
    /// `Icon`, else the PlayerMouse's (Inference: UserInputService's wins
    /// when both are set). Else
    /// Roblox's hand over a GUI button, the ClickDetector hand over a
    /// ClickDetector (Observed live), an I-beam over a TextBox
    /// (Inference), or Roblox's arrow (docs/behavior/gui/input.md#the-cursor).
    pub fn cursor_look(&self) -> CursorLook {
        let w = self.runtime.world();
        let uis = w.dom.find_service("UserInputService");
        if uis.is_some_and(|u| w.dom.bool_property(u, "MouseIconEnabled") == Some(false)) {
            return CursorLook::Hidden;
        }
        let mice = (w.input.tool_mice.iter().map(|&(_, m)| m)).chain(w.input.mouse_object);
        let icons = (uis
            .and_then(|u| w.dom.content_property(u, "MouseIconContent"))
            .into_iter())
        .chain(mice.filter_map(|m| w.dom.content_property(m, "IconContent")));
        if let Some(url) = icons.into_iter().find(|url| !url.is_empty()) {
            return CursorLook::icon(url);
        }
        let image = match self.hovered_press_target() {
            Some(id) if w.dom.is_a(id, "TextBox") => IBEAM,
            Some(_) => HAND,
            None if self.input.hover.is_some() => return CursorLook::DetectorHand,
            None => ARROW,
        };
        CursorLook::Image(image.to_owned())
    }

    /// The image at `url` if the GUI has loaded it, asking for it the
    /// first time.
    pub fn gui_image(&self, url: &str) -> Option<rook_gui::GuiImage> {
        self.runtime.world_mut().gui.image(url)
    }
}
