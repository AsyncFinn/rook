//! Hearing sounds: one mixer, resampling every sound to the output
//! device's rate, and the device itself.
//!
//! - [`Mixer`] holds what's playing and mixes it into a buffer. It knows
//!   nothing of devices, so it is tested without one.
//! - [`Output`] owns the output device (through `cpal`) and a mixer the
//!   device's thread pulls from. It opens the device the first time
//!   something is to be heard, so a silent place never opens one.
//! - [`spatial`] says how loud and on which side a 3D sound is heard.
//! - [`effects`] are the sound effects Sounds, SoundGroups and the audio
//!   API share. They don't need a device, so the engine uses them too
//!   (with the `output` feature off) for `PlaybackLoudness`.
//! - [`graph`] renders the audio API's graph (AudioPlayers, effects,
//!   emitters, listeners, outputs and analyzers joined by Wires); the
//!   mixer runs it for the device, and the engine runs it to measure
//!   AudioAnalyzers.
//!
//! The engine keeps each sound's state (`Playing`, `TimePosition`); the
//! player hands the playing ones, their SoundGroups and the audio graph
//! to `Output::sync` every frame. See
//! `docs/behavior/audio.md`.

pub mod effects;
pub mod graph;
mod mixer;
#[cfg(feature = "output")]
mod output;
pub mod spatial;
mod voice;

pub use effects::Effect;
pub use mixer::{Group, Mix, Mixer, Play};
#[cfg(feature = "output")]
pub use output::Output;

/// The environment variable that turns audio off (`ROOK_AUDIO=off`).
pub const AUDIO_VAR: &str = "ROOK_AUDIO";

/// Whether audio is on: everything but `ROOK_AUDIO=off`.
pub fn enabled_by_env() -> bool {
    !std::env::var(AUDIO_VAR).is_ok_and(|v| v.eq_ignore_ascii_case("off"))
}
