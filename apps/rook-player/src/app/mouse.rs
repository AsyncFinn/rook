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
            MouseLock::Center => self.hold(Some(center)),
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
        // Looking only happens while flying, and the game holds the mouse
        // only while it isn't, so one never lets go of the other's hold.
        let Some(at) = at else {
            let _ = gfx.window.set_cursor_grab(CursorGrabMode::None);
            return;
        };
        // A lock clips the cursor to the pixel it's on (winit on Windows),
        // and a warp can't leave the clip: let go first, then lock again
        // where it lands.
        let _ = gfx.window.set_cursor_grab(CursorGrabMode::None);
        self.warp(at);
        let Some(gfx) = &self.gfx else {
            return;
        };
        if gfx.window.set_cursor_grab(CursorGrabMode::Locked).is_err() {
            let _ = gfx.window.set_cursor_grab(CursorGrabMode::Confined);
            self.anchor = Some(at);
        }
    }

    /// Put the system cursor at `at`. The one place the window moves it,
    /// so the game always reads the mouse where it is: a lock to the
    /// center puts Roblox's GetMouseLocation and Mouse.Hit there, not
    /// where the mouse was when the lock came. Best effort, as grabbing is.
    fn warp(&mut self, at: PhysicalPosition<f64>) {
        let Some(gfx) = &self.gfx else {
            return;
        };
        #[expect(
            clippy::disallowed_methods,
            reason = "the one warp, which tells the game"
        )]
        let _ = gfx.window.set_cursor_position(at);
        self.cursor = at;
        let (x, y) = (at.x as f32, at.y as f32);
        if self.game.view().mouse_position() != (x, y) {
            self.game.input(InputEvent::MouseMove { x, y });
        }
    }

    /// Whether the cursor is held, so its moves aren't the player's.
    pub(crate) fn mouse_held(&self) -> bool {
        self.looking || self.held != MouseLock::Free
    }

    /// The mouse moved by `delta` since the last frame (raw device units,
    /// about a pixel each): the flying camera turns, or the game hears it,
    /// and a confined cursor is marked to go back.
    pub(crate) fn mouse_motion(&mut self, delta: (f64, f64)) {
        if delta == (0.0, 0.0) {
            return;
        }
        if self.held != MouseLock::Free {
            let (dx, dy) = (delta.0 as f32, delta.1 as f32);
            self.game.input(InputEvent::MouseMotion { dx, dy });
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
        self.strayed |= self.anchor.is_some();
    }

    /// Put a confined cursor that moved back where it's held, once a
    /// frame: the camera reads raw motion, so the cursor need only stay in
    /// the window, and warping it on every raw event (a thousand a second
    /// from a gaming mouse) floods the window with moves, as SDL's
    /// relative mode avoids.
    pub(crate) fn return_cursor(&mut self) {
        if !std::mem::take(&mut self.strayed) {
            return;
        }
        if let Some(anchor) = self.anchor {
            self.warp(anchor);
        }
    }
}
