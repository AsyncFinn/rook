//! Holding the mouse: still while the flying camera turns with the right
//! button, and as the game asks (`UserInputService.MouseBehavior`, through
//! [`rook_engine::DataModel::mouse_lock`]), where its movement reaches
//! scripts as `Delta`s while the cursor stays put.

use rook_engine::MouseLock;

use crate::*;

impl App {
    /// Start or stop looking around with the right mouse button: the
    /// cursor hides and stays where it was, as in Studio, instead of
    /// wandering out of the window while the camera turns.
    pub(crate) fn look(&mut self, on: bool) {
        self.looking = on;
        // Looking hides the cursor; whatever ends it shows it again.
        self.cursor_shown.look = None;
        if let Some(gfx) = &self.gfx {
            gfx.window.set_cursor_visible(!on);
        }
        self.hold(on.then_some(self.cursor));
    }

    /// Hold the mouse as the game asks, once per frame: at the window's
    /// center, where it is, or not at all. Only a focused window holds it.
    pub(crate) fn follow_mouse_lock(&mut self) {
        let Some(gfx) = &self.gfx else {
            return;
        };
        let want = if self.flying || !gfx.window.has_focus() {
            MouseLock::Free
        } else {
            self.game.view().mouse_lock()
        };
        if want == self.held {
            return;
        }
        self.held = want;
        let size = gfx.window.inner_size();
        let center =
            PhysicalPosition::new(f64::from(size.width) / 2.0, f64::from(size.height) / 2.0);
        match want {
            MouseLock::Free => self.hold(None),
            MouseLock::Center => {
                self.cursor = center;
                self.hold(Some(center));
            }
            MouseLock::InPlace => self.hold(Some(self.cursor)),
        }
    }

    /// Keep the cursor at `at`, or let it go. Best effort: platforms
    /// support different grab modes. Locked holds it still where the
    /// platform can; otherwise it's confined and put back after each move.
    fn hold(&mut self, at: Option<PhysicalPosition<f64>>) {
        self.anchor = None;
        let Some(gfx) = &self.gfx else {
            return;
        };
        let window = &gfx.window;
        // Looking only happens while flying, and the game holds the mouse
        // only while it isn't, so one never lets go of the other's hold.
        let Some(at) = at else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            return;
        };
        let _ = window.set_cursor_position(at);
        if window.set_cursor_grab(CursorGrabMode::Locked).is_err() {
            let _ = window.set_cursor_grab(CursorGrabMode::Confined);
            self.anchor = Some(at);
        }
    }

    /// Whether the cursor is held, so its moves aren't the player's.
    pub(crate) fn mouse_held(&self) -> bool {
        self.looking || self.held != MouseLock::Free
    }

    /// The mouse moved by `delta` (raw device units, about a pixel each):
    /// the flying camera turns, or the game hears it, and a confined
    /// cursor goes back.
    pub(crate) fn mouse_motion(&mut self, delta: (f64, f64)) {
        if self.held != MouseLock::Free {
            self.pending_motion.0 += delta.0 as f32;
            self.pending_motion.1 += delta.1 as f32;
        } else if self.looking {
            // Radians per pixel at Roblox's default sensitivity of 1.
            const TURN: f32 = 0.003;
            let (across, up) = self.player.mouse_sensitivity;
            let invert = if self.player.camera_y_inverted {
                -1.0
            } else {
                1.0
            };
            self.camera.yaw -= delta.0 as f32 * TURN * across;
            self.camera.pitch -= delta.1 as f32 * TURN * up * invert;
            self.camera.update_rotation();
        } else {
            return;
        }
        if let (Some(anchor), Some(gfx)) = (self.anchor, &self.gfx) {
            // Best effort, as grabbing is.
            let _ = gfx.window.set_cursor_position(anchor);
        }
    }
}
