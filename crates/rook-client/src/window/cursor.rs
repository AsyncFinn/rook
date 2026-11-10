//! The cursor: drawn into the frame at the mouse, as Roblox draws its own
//! (Roblox's images from the user's install, the icon scripts set, Rook's
//! ClickDetector hand), with the window's own hidden over it. The window
//! shows one of the system's instead for `rbxasset://SystemCursors`, and
//! while an image loads or without an install.

use rook_engine::{CursorLook, SystemCursor};
use winit::window::CursorIcon;

use super::*;

/// What the window's own cursor shows over the window, and whether the
/// mouse is in it.
#[derive(Debug, Default)]
pub(crate) struct Shown {
    /// What was last shown; `None` shows it again.
    pub(crate) look: Option<Showing>,
    /// The mouse is over the window, so the frame draws the cursor.
    pub(crate) inside: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Showing {
    /// Hidden: the frame draws the cursor, or there's none.
    Hidden,
    System(CursorIcon),
}

impl App {
    /// The cursor the game asks for, hidden while looking around, and
    /// where the mouse is when it's over the window.
    pub(crate) fn cursor_wanted(&self) -> (CursorLook, Option<(f32, f32)>) {
        let look = match self.looking {
            true => CursorLook::Hidden,
            false => self.game.view().cursor_look(),
        };
        let at = self
            .cursor_shown
            .inside
            .then(|| self.game.view().mouse_position());
        (look, at)
    }

    /// Show `look` with the window's own cursor, once a frame, unless the
    /// frame draws it (`drawn`): the one place the cursor changes, so what
    /// it shows is always what the game and the look say now.
    pub(crate) fn show_system_cursor(&mut self, look: &CursorLook, drawn: bool) {
        let Some(gfx) = &self.gfx else {
            return;
        };
        let want = match look {
            _ if drawn => Showing::Hidden,
            CursorLook::Hidden => Showing::Hidden,
            CursorLook::System(cursor) => Showing::System(system_icon(*cursor)),
            CursorLook::Image(_) | CursorLook::DetectorHand => match look.stand_in() {
                Some(CursorLook::System(cursor)) => Showing::System(system_icon(cursor)),
                _ => Showing::System(CursorIcon::Default),
            },
        };
        if self.cursor_shown.look == Some(want) {
            return;
        }
        let window = &gfx.window;
        #[expect(
            clippy::disallowed_methods,
            reason = "the one place the cursor changes"
        )]
        match want {
            Showing::Hidden => window.set_cursor_visible(false),
            Showing::System(icon) => {
                window.set_cursor_visible(true);
                window.set_cursor(icon);
            }
        }
        self.cursor_shown.look = Some(want);
    }
}

fn system_icon(cursor: SystemCursor) -> CursorIcon {
    match cursor {
        SystemCursor::Arrow => CursorIcon::Default,
        SystemCursor::PointingHand => CursorIcon::Pointer,
        SystemCursor::IBeam => CursorIcon::Text,
        SystemCursor::OpenHand => CursorIcon::Grab,
        SystemCursor::ClosedHand => CursorIcon::Grabbing,
    }
}
