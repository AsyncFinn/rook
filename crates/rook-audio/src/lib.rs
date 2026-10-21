//! Hearing sounds: one mixer, resampling every sound to the output
//! device's rate, and the device itself (`docs/behavior/audio/playback.md`).
//!
//! - [`Mixer`] mixes what's playing into a buffer; it knows nothing of
//!   devices, so it is tested without one.
//! - [`Output`] owns the device (through `cpal`), opened the first time
//!   something is to be heard, and a mixer the device's thread pulls from.
//! - [`spatial`]: how loud and on which side a 3D sound is heard.
//! - [`effects`]: the sound effects Sounds, SoundGroups and the audio API
//!   share; the engine uses them too, without a device, for
//!   `PlaybackLoudness`.
//! - [`graph`]: the audio API's graph, run by the mixer for the device and
//!   by the engine to measure AudioAnalyzers.

pub mod effects;
pub mod graph;
mod mixer;
#[cfg(feature = "output")]
mod output;
pub mod spatial;
mod voice;

pub use mixer::{Group, Mix, Mixer, Play, SAMPLE_RATE};
#[cfg(feature = "output")]
pub use output::Output;

/// Which output device to play through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Device {
    /// The system's default output device.
    Default,
    /// The output device with this name.
    Named(String),
}
