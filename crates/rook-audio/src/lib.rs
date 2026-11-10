//! Hearing sounds (`docs/behavior/audio/playback.md`). [`Mixer`] knows
//! nothing of devices, so it is tested without one; [`Output`] owns the
//! device and opens it the first time something is to be heard.

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Device {
    /// The system's default output device.
    Default,
    Named(String),
}
