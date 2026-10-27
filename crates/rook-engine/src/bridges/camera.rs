//! What the window takes from the game's camera: whether a script drives
//! `Workspace.CurrentCamera` (then the view is drawn from it), and how the
//! mouse is held (`UserInputService.MouseBehavior`). See
//! docs/behavior/characters/camera.md.

use glam::Vec3;
use rook_types::CFrame;

use crate::*;

/// How the window holds the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseLock {
    Free,
    /// At the window's center (first person, shift lock).
    Center,
    /// Where it was (turning the camera with the right button).
    InPlace,
}

/// The view a script set on the current camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraView {
    pub cframe: CFrame,
    /// Vertical, in degrees.
    pub fov: f32,
    /// `Camera.Focus`'s position.
    pub focus: Vec3,
}

impl DataModel {
    /// The current camera's view when a script drives it: a Scriptable
    /// camera, or the local player's character with a camera script in
    /// PlayerScripts (Roblox's PlayerModule or a place's own CameraScript).
    /// Otherwise the window flies its own camera.
    pub fn scripted_camera(&self) -> Option<CameraView> {
        let camera = self.current_camera()?;
        let w = self.runtime.world();
        let dom = &w.dom;
        let scriptable = dom.enum_item_property(camera, "CameraType") == Some("Scriptable");
        let followed = dom.local_player().is_some_and(|p| {
            let scripts = dom.find_first_child(p, "PlayerScripts", false);
            let has =
                |name| scripts.is_some_and(|s| dom.find_first_child(s, name, false).is_some());
            dom.ref_property(p, "Character").is_some()
                && (has("PlayerModule") || has("CameraScript"))
        });
        (scriptable || followed).then(|| {
            let cframe = dom.drawn_cframe(camera).unwrap_or(CFrame::IDENTITY);
            // Drawn moved with its subject, Focus with it.
            let shift = dom
                .cframe_property(camera, "CFrame")
                .map_or(Vec3::ZERO, |cf| cframe.position - cf.position);
            CameraView {
                cframe,
                fov: dom.float_property(camera, "FieldOfView").unwrap_or(70.0),
                focus: dom
                    .cframe_property(camera, "Focus")
                    .map_or(cframe.position, |f| f.position + shift),
            }
        })
    }

    /// How the window should hold the mouse: as
    /// `UserInputService.MouseBehavior` says, unless a shown GuiButton is
    /// `Modal` or a TextBox has focus (Inference: typing frees it).
    pub fn mouse_lock(&self) -> MouseLock {
        // Asked first: it borrows the World itself.
        let typing = self.runtime.focused_text_box().is_some();
        let w = self.runtime.world();
        let behavior = w
            .dom
            .find_service("UserInputService")
            .and_then(|uis| w.dom.enum_item_property(uis, "MouseBehavior"));
        let lock = match behavior {
            Some("LockCenter") => MouseLock::Center,
            Some("LockCurrentPosition") => MouseLock::InPlace,
            _ => return MouseLock::Free,
        };
        if typing || w.gui.modal_shown(&w.dom) {
            return MouseLock::Free;
        }
        lock
    }
}
