//! What the window takes from the game's camera: whether a script drives
//! `Workspace.CurrentCamera` (then the view is drawn from it), and how the
//! mouse is held (`UserInputService.MouseBehavior`). See
//! docs/behavior/characters/camera.md.

use glam::Vec3;
use rook_types::CFrame;

use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseLock {
    Free,
    /// At the window's center (first person, shift lock).
    Center,
    /// Where it was (turning the camera with the right button).
    InPlace,
}

/// `Workspace.CurrentCamera` as the place was saved with it. Plain values,
/// so a server thread can hand it to the window's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SavedCamera {
    pub cframe: CFrame,
    /// Vertical, in degrees.
    pub fov_y: f32,
}

impl SavedCamera {
    pub fn read(dm: &DataModel) -> Option<Self> {
        let world = dm.runtime().world();
        let dom = &world.dom;
        let camera = dm.current_camera()?;
        Some(Self {
            cframe: dom.cframe_property(camera, "CFrame")?,
            fov_y: dom.float_property(camera, "FieldOfView").unwrap_or(70.0),
        })
    }
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

/// The Camera properties read each frame, resolved once.
struct CameraIds {
    camera_type: rook_reflection::PropertyId,
    cframe: rook_reflection::PropertyId,
    focus: rook_reflection::PropertyId,
    fov: rook_reflection::PropertyId,
}

fn camera_ids() -> &'static CameraIds {
    static IDS: std::sync::LazyLock<CameraIds> = std::sync::LazyLock::new(|| {
        let db = rook_reflection::database();
        let camera = db.class_by_name("Camera").expect("Camera").id;
        let id = |name| db.find_canonical_property(camera, name).expect(name).id;
        CameraIds {
            camera_type: id("CameraType"),
            cframe: id("CFrame"),
            focus: id("Focus"),
            fov: id("FieldOfView"),
        }
    });
    &IDS
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
        let ids = camera_ids();
        let cframe_of = |property| dom.read_property(camera, property, Value::as_cframe);
        let scriptable = dom.enum_item(camera, ids.camera_type) == Some("Scriptable");
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
            let shift =
                cframe_of(ids.cframe).map_or(Vec3::ZERO, |cf| cframe.position - cf.position);
            CameraView {
                cframe,
                fov: (dom.read_property(camera, ids.fov, Value::as_number))
                    .map_or(70.0, |f| f as f32),
                focus: cframe_of(ids.focus).map_or(cframe.position, |f| f.position + shift),
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
