//! Sound's methods, and the audio API's: players, emitters, listeners, wires and analyzers.

use crate::api::methods::{RawMethod, first};

mod audio_api;
pub(crate) mod sound;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(&[sound::native, audio_api::native], owner, name)
}
