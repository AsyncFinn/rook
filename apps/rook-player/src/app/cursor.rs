//! The window's cursor: Roblox's own images from the user's install, the
//! icon scripts set (`Mouse.Icon`, `UserInputService.MouseIcon`), one of
//! the system's, or hidden. Without an install, the system's stand in.

use std::collections::HashMap;

use rook_engine::{CursorLook, SystemCursor};
use winit::window::CustomCursor;

use crate::*;

/// What the cursor shows, and the images made into cursors so far by URL.
#[derive(Debug, Default)]
pub(crate) struct Shown {
    /// What was last shown; `None` shows it again.
    pub(crate) look: Option<Showing>,
    icons: HashMap<String, CustomCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Showing {
    Hidden,
    Image(String),
    System(CursorIcon),
}

impl App {
    /// Show the cursor the game asks for.
    pub(crate) fn show_cursor(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_none() {
            return;
        }
        let look = self.game.view().cursor_look();
        let want = self.showing(event_loop, &look);
        if self.cursor_shown.look.as_ref() == Some(&want) {
            return;
        }
        let Some(gfx) = &self.gfx else {
            return;
        };
        let window = &gfx.window;
        match &want {
            Showing::Hidden => window.set_cursor_visible(false),
            Showing::Image(url) => {
                window.set_cursor_visible(!self.looking);
                window.set_cursor(self.cursor_shown.icons[url].clone());
            }
            Showing::System(icon) => {
                window.set_cursor_visible(!self.looking);
                window.set_cursor(*icon);
            }
        }
        self.cursor_shown.look = Some(want);
    }

    /// How to show `look`, or its stand-in while its image loads.
    fn showing(&mut self, event_loop: &ActiveEventLoop, look: &CursorLook) -> Showing {
        match look {
            CursorLook::Hidden => Showing::Hidden,
            CursorLook::System(cursor) => Showing::System(system_icon(*cursor)),
            CursorLook::Image(url) if self.make_icon(event_loop, url) => {
                Showing::Image(url.clone())
            }
            CursorLook::Image(_) => match look.stand_in() {
                Some(stand_in) => self.showing(event_loop, &stand_in),
                None => Showing::System(CursorIcon::Default),
            },
        }
    }

    /// Whether the image at `url` is a cursor yet, making it once it has
    /// loaded.
    fn make_icon(&mut self, event_loop: &ActiveEventLoop, url: &str) -> bool {
        if self.cursor_shown.icons.contains_key(url) {
            return true;
        }
        let made = (self.game.view().gui_image(url))
            .and_then(|image| self.scene.image_pixels(image))
            .and_then(|(w, h, rgba)| {
                let (w16, h16) = (u16::try_from(w).ok()?, u16::try_from(h).ok()?);
                // Inference: Roblox centers an icon on the cursor.
                let source = CustomCursor::from_rgba(rgba, w16, h16, w16 / 2, h16 / 2).ok()?;
                Some(event_loop.create_custom_cursor(source))
            });
        match made {
            Some(made) => {
                self.cursor_shown.icons.insert(url.to_owned(), made);
                true
            }
            None => false,
        }
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
