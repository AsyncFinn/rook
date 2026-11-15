//! The graph run on the audio thread: its nodes kept in step with the
//! graph, sorted so each runs after what feeds it, and rendered.

use super::*;

#[derive(Debug)]
pub(super) struct State<K> {
    /// What the node puts out this block (an output's or analyzer's
    /// input, for those).
    pub(super) buffer: Vec<Frame>,
    pub(super) chain: Chain,
    pub(super) voice: Option<Voice<K>>,
    /// A listener's gains per emitter last block, ramped from.
    pub(super) gains: FxHashMap<K, [f32; 2]>,
    pub(super) meter: Option<Meter>,
}

/// Renders a [`Graph`] at one rate, keeping each node's state (a player's
/// position, effects' tails, analyzers' history) from block to block.
#[derive(Debug)]
pub struct Runner<K> {
    pub(super) rate: u32,
    pub(super) graph: Graph<K>,
    /// Node indices in the order they run: every node after what feeds it.
    pub(super) order: Vec<usize>,
    /// Each node's inputs: the feeding node's index, and whether it goes to
    /// the sidechain.
    pub(super) inputs: Vec<Vec<(usize, bool)>>,
    pub(super) states: FxHashMap<K, State<K>>,
    pub(super) scratch: Vec<Frame>,
    pub(super) key: Vec<Frame>,
    /// The [`Graph::topology`] last synced.
    pub(super) synced: Option<u64>,
}

impl<K: Hash + Eq + Copy> Runner<K> {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate.max(1),
            graph: Graph::default(),
            order: Vec::new(),
            inputs: Vec::new(),
            states: FxHashMap::default(),
            scratch: Vec::new(),
            key: Vec::new(),
            synced: None,
        }
    }

    /// Take the listeners' gains of `graph`, of the topology last synced
    /// (another is left for [`Runner::sync`]).
    pub fn follow(&mut self, graph: &Graph<K>) {
        if self.synced != Some(graph.topology) {
            return self.sync(graph);
        }
        for (mine, theirs) in self.graph.nodes.iter_mut().zip(&graph.nodes) {
            if let (NodeKind::Listener(mine), NodeKind::Listener(theirs)) =
                (&mut mine.kind, &theirs.kind)
            {
                mine.clone_from(theirs);
            }
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn is_empty(&self) -> bool {
        self.graph.is_empty()
    }

    /// Render `graph` from now on. One of the topology last synced only
    /// changes listeners' gains ([`Runner::follow`]); any other starts new
    /// nodes, drops removed ones and orders them again, and players keep
    /// their own position until played again or seeked (`generation`).
    pub fn sync(&mut self, graph: &Graph<K>) {
        if self.synced == Some(graph.topology) {
            self.follow(graph);
            return;
        }
        self.synced = Some(graph.topology);
        let index: FxHashMap<K, usize> = graph
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
                gains: FxHashMap::default(),
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
                    state.gains.clear();
                    state.gains.extend(heard.iter().copied());
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

    pub fn meter(&self, key: K) -> Option<&Meter> {
        self.states.get(&key)?.meter.as_ref()
    }
}

/// The nodes in an order where each comes after the nodes feeding it
/// (Kahn's). Nodes in a loop, which only a listener hearing an emitter fed
/// from it can make (wires that would close a loop aren't connected), come
/// last, hearing what the emitter had the block before.
pub(super) fn order(count: usize, edges: &[(usize, usize)]) -> Vec<usize> {
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
