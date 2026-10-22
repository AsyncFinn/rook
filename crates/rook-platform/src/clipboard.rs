//! The clipboard: text a TextBox copies and pastes.

use std::fmt::Debug;

pub trait Clipboard: Send + Sync + Debug {
    /// The text on it, if it holds text.
    fn text(&self) -> Option<String>;
    fn set_text(&self, text: &str);
}

/// The system's clipboard through arboard, opened for each use: holding
/// it open would keep other programs off it on some systems.
#[cfg(feature = "native")]
#[derive(Debug)]
pub struct System;

#[cfg(feature = "native")]
impl Clipboard for System {
    fn text(&self) -> Option<String> {
        arboard::Clipboard::new().ok()?.get_text().ok()
    }

    fn set_text(&self, text: &str) {
        // Best effort: a clipboard another program holds just misses it.
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(text);
        }
    }
}
