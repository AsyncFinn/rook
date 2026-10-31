//! The cursor: drawn into the frame at the mouse, as Roblox draws its own
//! (Roblox's images from the user's install, the icon scripts set, Rook's
//! ClickDetector hand), with the window's own hidden over it. The window
//! shows one of the system's instead for `rbxasset://SystemCursors`, and
//! while an image loads or without an install.

use rook_engine::{CursorLook, SystemCursor};

use crate::*;

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
    /// Show the cursor the game asks for, hidden while looking around, once
    /// a frame: the one place the cursor changes, so what it shows is
    /// always what the game and the look say now.
    pub(crate) fn show_cursor(&mut self) {
        let Some(gfx) = &mut self.gfx else {
            return;
        };
        let look = match self.looking {
            true => CursorLook::Hidden,
            false => self.game.view().cursor_look(),
        };
        let at = self
            .cursor_shown
            .inside
            .then(|| self.game.view().mouse_position());
        let drawn = (self.scene).show_cursor(self.game.view(), &mut gfx.renderer, &look, at);
        let want = match &look {
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
