//! The window's cursor: a pointing hand over a ClickDetector, or the icon
//! scripts set (`Mouse.Icon`, `UserInputService.MouseIcon`), or hidden.

use std::collections::HashMap;

use rook_engine::CursorLook;
use winit::window::CustomCursor;

use crate::*;

/// What the cursor shows, and the icons made so far by URL.
#[derive(Debug, Default)]
pub(crate) struct Shown {
    /// What was last shown, with whether it pointed; `None` shows it
    /// again.
    pub(crate) look: Option<(CursorLook, bool)>,
    icons: HashMap<String, CustomCursor>,
}

impl App {
    /// Show the cursor the game asks for. An icon still loading shows the
    /// default arrow until it arrives.
    pub(crate) fn show_cursor(&mut self, event_loop: &ActiveEventLoop) {
        let Some(gfx) = &self.gfx else {
            return;
        };
        let look = self.game.view().cursor_look();
        let pointing = self.game.hovering_click_detector();
        let mut want = (look, pointing);
        if let (CursorLook::Image(url), _) = &want
            && !self.cursor_shown.icons.contains_key(url)
        {
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
                    self.cursor_shown.icons.insert(url.clone(), made);
                }
                None => want.0 = CursorLook::Default,
            }
        }
        if self.cursor_shown.look.as_ref() == Some(&want) {
            return;
        }
        let window = &gfx.window;
        match &want.0 {
            CursorLook::Hidden => window.set_cursor_visible(false),
            CursorLook::Image(url) => {
                window.set_cursor_visible(!self.looking);
                window.set_cursor(self.cursor_shown.icons[url].clone());
            }
            CursorLook::Default => {
                window.set_cursor_visible(!self.looking);
                window.set_cursor(if pointing {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                });
            }
        }
        self.cursor_shown.look = Some(want);
    }
}
