//! The audio API's graph (`docs/behavior/audio/audio-api.md`). The engine
//! describes it as a [`Graph`], anew when its nodes or wires change and
//! otherwise with only listeners' gains moved, and a [`Runner`] renders it
//! block by block. Players play through the same [`Voice`] as Sounds, and effects
//! through the same [`Chain`], so the two APIs sound alike.
//!
//! Every stream is stereo (Approximation: Studio's streams have their own
//! channel layouts, see [`meter`]).

pub mod meter;
mod runner;

pub use runner::Runner;

use rustc_hash::FxHashMap;
use std::hash::Hash;
use std::sync::Arc;

use rook_content::decode::audio::Audio;

use crate::effects::{Chain, Effect, Frame};
use crate::voice::{self, Voice};
pub use meter::Meter;

#[derive(Debug, Clone)]
pub struct Source {
    pub audio: Arc<Audio>,
    /// `TimePosition`, seconds.
    pub position: f64,
    /// `Volume`, a linear gain (Confirmed: 0.5 halved a recording's peak
    /// exactly).
    pub volume: f32,
    pub speed: f32,
    pub looped: bool,
    /// Changes with every `Play`, so a restart is heard as one.
    pub generation: u32,
}

#[derive(Debug, Clone)]
pub enum NodeKind<K> {
    /// An AudioPlayer, with what it plays if it is playing a loaded file.
    Player(Option<Source>),
    /// An effect, or `None` for one that passes its input through as it is
    /// (bypassed, or of a kind Rook doesn't play).
    Effect(Option<Effect>),
    /// An AudioEmitter: what is wired into it is heard by listeners.
    Emitter,
    /// An AudioListener: each emitter it hears with that emitter's left and
    /// right gains for it.
    Listener(Vec<(K, [f32; 2])>),
    /// An AudioDeviceOutput whose input is heard.
    Output,
    /// An AudioAnalyzer: its input is measured.
    Analyzer,
}

#[derive(Debug, Clone)]
pub struct Node<K> {
    pub key: K,
    pub kind: NodeKind<K>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wire<K> {
    pub from: K,
    pub to: K,
    /// Into an AudioCompressor's **Sidechain** pin rather than its input.
    pub sidechain: bool,
}

#[derive(Debug, Clone)]
pub struct Graph<K> {
    /// Changes whenever the nodes or wires do, but for listeners' gains:
    /// a runner given the same one again only follows those
    /// ([`Runner::follow`]).
    pub topology: u64,
    pub nodes: Vec<Node<K>>,
    pub wires: Vec<Wire<K>>,
}

impl<K> Default for Graph<K> {
    fn default() -> Self {
        Self {
            topology: 0,
            nodes: Vec::new(),
            wires: Vec::new(),
        }
    }
}

impl<K> Graph<K> {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Whether anything in it could be heard: a device output.
    pub fn has_output(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Output))
    }
}

#[cfg(test)]
mod tests;
