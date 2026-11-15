//! Sound's methods, and the audio API's: players, emitters, listeners, wires and analyzers.

use super::*;

mod audio_api;
pub(crate) mod sound;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(&[sound::native, audio_api::native], owner, name)
}
