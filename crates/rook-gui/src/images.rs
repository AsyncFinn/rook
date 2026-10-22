//! The images GUI draws (ImageLabels', ImageButtons' and scroll bars'),
//! by content URL. The drawing side loads them ([`Gui::take_image_requests`],
//! [`Gui::image_arrived`]); until one arrives, what shows it draws nothing.
//!
//! [`Gui::take_image_requests`]: crate::Gui::take_image_requests
//! [`Gui::image_arrived`]: crate::Gui::image_arrived

use rustc_hash::FxHashMap;

/// A loaded image: its file, by the BLAKE3 hash of its bytes, and its size
/// in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuiImage {
    pub hash: [u8; 32],
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Loading,
    Loaded(GuiImage),
    Failed,
}

#[derive(Debug, Default)]
pub(crate) struct Images {
    states: FxHashMap<String, State>,
    /// Asked for and not yet taken by the loader.
    requests: Vec<String>,
}

impl Images {
    /// The image at `url` if it has loaded, asking for it the first time.
    pub(crate) fn get(&mut self, url: &str) -> Option<GuiImage> {
        if url.is_empty() {
            return None;
        }
        match self.states.get(url) {
            Some(State::Loaded(image)) => Some(*image),
            Some(_) => None,
            None => {
                self.states.insert(url.to_owned(), State::Loading);
                self.requests.push(url.to_owned());
                None
            }
        }
    }

    /// Whether `url` has loaded (`IsLoaded`), without asking for it.
    pub(crate) fn is_loaded(&self, url: &str) -> bool {
        matches!(self.states.get(url), Some(State::Loaded(_)))
    }

    pub(crate) fn take_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.requests)
    }

    /// A requested image arrived (`None`: it can't be had). Whether that
    /// changes what's drawn.
    pub(crate) fn arrived(&mut self, url: &str, image: Option<GuiImage>) -> bool {
        let state = image.map_or(State::Failed, State::Loaded);
        self.states.insert(url.to_owned(), state) != Some(state)
    }
}
