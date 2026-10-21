//! The audio API's graph: AudioPlayers, effects, AudioEmitters,
//! AudioListeners, AudioDeviceOutputs and AudioAnalyzers joined by Wires
//! (`docs/behavior/audio/audio-api.md`).
//!
//! The engine describes the graph each frame as a [`Graph`]: its nodes,
//! with what each player is playing and what each listener hears, and its
//! connected wires. A [`Runner`] renders it block by block. The mixer runs
//! one for the output device; the engine runs another on clients to
//! measure what reaches AudioAnalyzers. Players play through the same
//! [`Voice`] as Sounds, and effects through the same [`Chain`].
//!
//! Every stream is stereo (Approximation: Studio's streams have their own
//! channel layouts, see [`meter`]).

pub mod meter;

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use rook_content::decode::audio::Audio;

use crate::effects::{Chain, Effect, Frame};
use crate::voice::{self, Voice};
pub use meter::Meter;

/// What an AudioPlayer is playing.
#[derive(Debug, Clone)]
pub struct Source {
    pub audio: Arc<Audio>,
    /// `TimePosition`, seconds.
    pub position: f64,
    /// `Volume`, a linear gain (Confirmed: 0.5 halved a recording's peak
    /// exactly).
    pub volume: f32,
    /// `PlaybackSpeed`.
    pub speed: f32,
    pub looped: bool,
    /// Changes with every `Play`, so a restart is heard as one.
    pub generation: u32,
}

/// One instance in the graph.
#[derive(Debug, Clone)]
pub enum NodeKind<K> {
    /// An AudioPlayer, with what it plays if it is playing a loaded file.
    Player(Option<Source>),
    /// An effect, or `None` for one that passes its input through as it
    /// is (bypassed, or one Rook doesn't play yet).
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

/// A connected Wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wire<K> {
    pub from: K,
    pub to: K,
    /// Into an AudioCompressor's **Sidechain** pin rather than its input.
    pub sidechain: bool,
}

#[derive(Debug, Clone)]
pub struct Graph<K> {
    pub nodes: Vec<Node<K>>,
    pub wires: Vec<Wire<K>>,
}

impl<K> Default for Graph<K> {
    fn default() -> Self {
        Self {
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

/// A node's running state.
#[derive(Debug)]
struct State<K> {
    /// What the node puts out this block (an output's or analyzer's
    /// input, for those).
    buffer: Vec<Frame>,
    chain: Chain,
    voice: Option<Voice<K>>,
    /// A listener's gains per emitter last block, ramped from.
    gains: HashMap<K, [f32; 2]>,
    meter: Option<Meter>,
}

/// Renders a [`Graph`] at one rate, keeping each node's state (a player's
/// position, effects' tails, analyzers' history) from block to block.
#[derive(Debug)]
pub struct Runner<K> {
    rate: u32,
    graph: Graph<K>,
    /// Node indices in the order they run: every node after what feeds it.
    order: Vec<usize>,
    /// Each node's inputs: the feeding node's index, and whether it goes to
    /// the sidechain.
    inputs: Vec<Vec<(usize, bool)>>,
    states: HashMap<K, State<K>>,
    scratch: Vec<Frame>,
    key: Vec<Frame>,
}

impl<K: Hash + Eq + Copy> Runner<K> {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate.max(1),
            graph: Graph::default(),
            order: Vec::new(),
            inputs: Vec::new(),
            states: HashMap::new(),
            scratch: Vec::new(),
            key: Vec::new(),
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn is_empty(&self) -> bool {
        self.graph.is_empty()
    }

    /// Render `graph` from now on: new nodes start, removed ones go, and
    /// players keep their own position until played again or seeked
    /// (`generation`).
    pub fn sync(&mut self, graph: &Graph<K>) {
        let index: HashMap<K, usize> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.key, i))
            .collect();
        self.states.retain(|k, _| index.contains_key(k));
        for node in &graph.nodes {
            let rate = self.rate;
            let state = self.states.entry(node.key).or_insert_with(|| State {
                buffer: Vec::new(),
                chain: Chain::new(rate),
                voice: None,
                gains: HashMap::new(),
                meter: None,
            });
            match &node.kind {
                NodeKind::Player(Some(s)) => {
                    let gains = [s.volume; 2];
                    let voice = state
                        .voice
                        .get_or_insert_with(|| Voice::new(&s.audio, s.position, gains, rate));
                    voice.follow(&s.audio, s.position, s.generation);
                    voice.target = gains;
                    voice.speed = s.speed;
                    voice.looped = s.looped;
                }
                NodeKind::Player(None) => state.voice = None,
                NodeKind::Effect(e) => state.chain.sync(e.as_slice()),
                NodeKind::Analyzer => {
                    state.meter.get_or_insert_with(Meter::default);
                }
                _ => {}
            }
        }
        self.inputs = vec![Vec::new(); graph.nodes.len()];
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for w in &graph.wires {
            if let (Some(&from), Some(&to)) = (index.get(&w.from), index.get(&w.to))
                && !self.inputs[to].contains(&(from, w.sidechain))
            {
                self.inputs[to].push((from, w.sidechain));
                edges.push((from, to));
            }
        }
        for (to, node) in graph.nodes.iter().enumerate() {
            if let NodeKind::Listener(heard) = &node.kind {
                edges.extend(heard.iter().filter_map(|(e, _)| Some((*index.get(e)?, to))));
            }
        }
        self.order = order(graph.nodes.len(), &edges);
        self.graph = graph.clone();
    }

    /// Render one block of `out.len()` frames, adding what device outputs
    /// receive into `out`.
    pub fn render(&mut self, out: &mut [Frame]) {
        if self.graph.is_empty() {
            return;
        }
        let frames = out.len();
        let rate = f64::from(self.rate);
        for &i in &self.order {
            let node = &self.graph.nodes[i];
            let Some(mut buffer) = self
                .states
                .get_mut(&node.key)
                .map(|s| std::mem::take(&mut s.buffer))
            else {
                continue;
            };
            buffer.clear();
            buffer.resize(frames, [0.0; 2]);
            let mut keyed = false;
            self.key.clear();
            self.key.resize(frames, [0.0; 2]);
            for &(from, sidechain) in &self.inputs[i] {
                let Some(src) = self.states.get(&self.graph.nodes[from].key) else {
                    continue;
                };
                let into = if sidechain {
                    keyed = true;
                    &mut self.key
                } else {
                    &mut buffer
                };
                for (a, b) in src.buffer.iter().zip(into.iter_mut()) {
                    b[0] += a[0];
                    b[1] += a[1];
                }
            }
            if let NodeKind::Listener(heard) = &node.kind {
                for (emitter, target) in heard {
                    let Some(src) = self.states.get(emitter) else {
                        continue;
                    };
                    let state = self.states.get(&node.key).expect("listener state");
                    let from = state.gains.get(emitter).copied().unwrap_or(*target);
                    voice::add(&src.buffer, &mut buffer, from, *target);
                }
            }
            let state = self.states.get_mut(&node.key).expect("node state");
            match &node.kind {
                NodeKind::Player(_) => {
                    if let Some(voice) = &mut state.voice {
                        self.scratch.clear();
                        self.scratch.resize(frames, [0.0; 2]);
                        // An ended voice stays, silent, until the engine
                        // stops or restarts the player.
                        voice.render(&mut self.scratch, rate);
                        voice::add(&self.scratch, &mut buffer, voice.gains, voice.target);
                        voice.gains = voice.target;
                    }
                }
                NodeKind::Effect(Some(_)) => state
                    .chain
                    .process_keyed(&mut buffer, keyed.then_some(&self.key[..])),
                NodeKind::Listener(heard) => {
                    state.gains = heard.iter().copied().collect();
                }
                NodeKind::Output => {
                    for (a, b) in buffer.iter().zip(out.iter_mut()) {
                        b[0] += a[0];
                        b[1] += a[1];
                    }
                }
                NodeKind::Analyzer => {
                    if let Some(meter) = &mut state.meter {
                        meter.feed(&buffer);
                    }
                }
                NodeKind::Effect(None) | NodeKind::Emitter => {}
            }
            state.buffer = buffer;
        }
    }

    /// An analyzer's meter.
    pub fn meter(&self, key: K) -> Option<&Meter> {
        self.states.get(&key)?.meter.as_ref()
    }
}

/// The nodes in an order where each comes after the nodes feeding it
/// (Kahn's). Nodes in a loop, which only a listener hearing an emitter fed
/// from it can make (wires that would close a loop aren't connected), come
/// last, hearing what the emitter had the block before.
fn order(count: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut incoming = vec![0usize; count];
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); count];
    for &(a, b) in edges {
        incoming[b] += 1;
        out[a].push(b);
    }
    let mut ready: Vec<usize> = (0..count).filter(|&i| incoming[i] == 0).rev().collect();
    let mut done = vec![false; count];
    let mut list = Vec::with_capacity(count);
    while let Some(i) = ready.pop() {
        done[i] = true;
        list.push(i);
        for &b in &out[i] {
            incoming[b] -= 1;
            if incoming[b] == 0 {
                ready.push(b);
            }
        }
    }
    list.extend((0..count).filter(|&i| !done[i]));
    list
}

#[cfg(test)]
mod tests;
