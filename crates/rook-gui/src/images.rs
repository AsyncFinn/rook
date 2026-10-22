//! The images GUI draws (ImageLabels', ImageButtons' and scroll bars'),
//! by content URL. The drawing side loads them ([`Gui::take_image_requests`],
//! [`Gui::image_arrived`]); until one arrives, what shows it draws nothing.
//!
//! [`Gui::take_image_requests`]: crate::Gui::take_image_requests
//! [`Gui::image_arrived`]: crate::Gui::image_arrived

use rook_dom::InstanceId;
use rustc_hash::{FxHashMap, FxHashSet};

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
    /// The objects that asked for each image still loading, whose trees
    /// draw again when it arrives.
    waiting: FxHashMap<String, FxHashSet<InstanceId>>,
}

impl Images {
    /// The image at `url` if it has loaded, asking for it the first time
    /// on behalf of object `by`.
    pub(crate) fn get(&mut self, url: &str, by: Option<InstanceId>) -> Option<GuiImage> {
        if url.is_empty() {
            return None;
        }
        match self.states.get(url) {
            Some(State::Loaded(image)) => return Some(*image),
            Some(State::Failed) => return None,
            Some(State::Loading) => {}
            None => {
                self.states.insert(url.to_owned(), State::Loading);
                self.requests.push(url.to_owned());
            }
        }
        if let Some(by) = by {
            self.waiting.entry(url.to_owned()).or_default().insert(by);
        }
        None
    }

    /// Whether `url` has loaded (`IsLoaded`), without asking for it.
    pub(crate) fn is_loaded(&self, url: &str) -> bool {
        matches!(self.states.get(url), Some(State::Loaded(_)))
    }

    pub(crate) fn take_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.requests)
    }

    /// A requested image arrived (`None`: it can't be had). The objects
    /// that were waiting for it.
    pub(crate) fn arrived(&mut self, url: &str, image: Option<GuiImage>) -> FxHashSet<InstanceId> {
        let state = image.map_or(State::Failed, State::Loaded);
        self.states.insert(url.to_owned(), state);
        self.waiting.remove(url).unwrap_or_default()
    }
}
