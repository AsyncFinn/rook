//! Mixing playing sounds into one stream: each sound through its own
//! effects, then its SoundGroups, then `SoundService.AmbientReverb`; and
//! the audio API's graph, whose device outputs are added after that.

mod bus;

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use rook_content::decode::audio::Audio;

use crate::effects::{Chain, Effect, Frame, Reverb};
use crate::graph::{Graph, Runner};
use crate::voice::{self, Voice};
use bus::Bus;

/// How deep SoundGroups may nest before the rest are taken as top-level
/// (a loop of groups can't be built in Roblox's tree, but a damaged file
/// shouldn't hang the mixer).
const MAX_DEPTH: usize = 32;

/// A mono Sound's level on each side: Studio centres it, 3 dB down
/// (measured on the output for global sounds at every Volume; a stereo
/// file plays at its own level). The 3D panning in [`crate::spatial`] is
/// relative to this. Applied after the sound's effects, as a channel's
/// panning comes after its effects (Inference). The audio API keeps a mono
/// stream mono, so its AudioPlayers don't take it.
const MONO_LEVEL: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// One sound to hear, as the engine sees it this frame.
#[derive(Debug, Clone)]
pub struct Play<K> {
    pub key: K,
    pub audio: Arc<Audio>,
    /// Seconds into the sound.
    pub position: f64,
    /// Linear gain for the left and right of the output: `Volume`, with a
    /// 3D sound's roll-off and panning ([`crate::spatial`]). Applied after
    /// the sound's effects.
    pub gains: [f32; 2],
    /// 1 is the sound's own rate.
    pub speed: f32,
    pub looped: bool,
    /// Changes when the sound is played again: the mixer then starts it
    /// over at `position`, however close it already was.
    pub generation: u32,
    /// The sound's own effects, in the order they run
    /// ([`crate::effects::ordered`]).
    pub effects: Vec<Effect>,
    /// The SoundGroup it plays through.
    pub group: Option<K>,
}

/// A SoundGroup: its sounds and the groups in it are mixed together, run
/// through its effects and scaled by its `Volume`, then go into the group
/// it is in.
#[derive(Debug, Clone)]
pub struct Group<K> {
    pub key: K,
    pub parent: Option<K>,
    pub volume: f32,
    pub effects: Vec<Effect>,
}

/// Everything to hear this frame.
#[derive(Debug, Clone)]
pub struct Mix<K> {
    pub sounds: Vec<Play<K>>,
    /// The groups the sounds play through, and the groups those are in.
    pub groups: Vec<Group<K>>,
    /// `SoundService.AmbientReverb`, added to every Sound.
    pub ambient: Option<Reverb>,
    /// The audio API's instances and wires.
    pub graph: Graph<K>,
}

impl<K> Default for Mix<K> {
    fn default() -> Self {
        Self {
            sounds: Vec::new(),
            groups: Vec::new(),
            ambient: None,
            graph: Graph::default(),
        }
    }
}

/// The rate mixes that aren't heard run at (recordings, analyzers): the
/// usual output device's. A device's mixer runs at the device's own rate.
pub const SAMPLE_RATE: u32 = 48_000;

/// The sounds playing now, mixed at `rate` frames per second.
#[derive(Debug)]
pub struct Mixer<K> {
    rate: u32,
    voices: HashMap<K, Voice<K>>,
    buses: HashMap<K, Bus<K>>,
    ambient: Chain,
    ambient_idle: usize,
    graph: Runner<K>,
    /// Reused buffers: one voice at a time, and the output before it is
    /// laid out on the device's channels.
    scratch: Vec<Frame>,
    master: Vec<Frame>,
}

impl<K: Hash + Eq + Copy> Mixer<K> {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate.max(1),
            voices: HashMap::new(),
            buses: HashMap::new(),
            ambient: Chain::new(rate),
            ambient_idle: 0,
            graph: Runner::new(rate),
            scratch: Vec::new(),
            master: Vec::new(),
        }
    }

    /// Voices still sounding (an ended one waits, silent, for the engine
    /// to stop or restart it).
    pub fn len(&self) -> usize {
        self.voices.values().filter(|v| !v.ended).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The last [`mix`](Self::mix) as left and right, before it was laid
    /// out on the device's channels.
    pub fn mixed(&self) -> &[Frame] {
        &self.master
    }

    /// Make the mixer play exactly `mix`: new sounds start at their
    /// position, ones no longer wanted stop, and ones still playing keep
    /// their own position until they are played again or seeked
    /// (`generation`).
    /// Groups no longer listed play out their effects' tails.
    pub fn sync(&mut self, mix: &Mix<K>) {
        let wanted = &mix.sounds;
        self.voices
            .retain(|k, _| wanted.iter().any(|p| p.key == *k));
        for p in wanted {
            let voice = self
                .voices
                .entry(p.key)
                .or_insert_with(|| Voice::new(&p.audio, p.position, p.gains, self.rate));
            voice.follow(&p.audio, p.position, p.generation);
            voice.target = p.gains;
            voice.speed = p.speed;
            voice.looped = p.looped;
            voice.group = p.group;
            voice.effects.sync(&p.effects);
        }
        for bus in self.buses.values_mut() {
            bus.listed = false;
        }
        for g in &mix.groups {
            let bus = self
                .buses
                .entry(g.key)
                .or_insert_with(|| Bus::new(g.parent, g.volume, self.rate));
            bus.listed = true;
            bus.parent = g.parent.filter(|p| *p != g.key);
            bus.target = g.volume;
            bus.effects.sync(&g.effects);
        }
        self.buses.retain(|_, b| b.listed || b.sounding());
        match &mix.ambient {
            Some(r) => self.ambient.sync(&[Effect::Reverb(*r)]),
            None => self.ambient.sync(&[]),
        }
        self.graph.sync(&mix.graph);
    }

    /// Mix into `out`, interleaved with `channels` channels, replacing what
    /// it held. The left side goes to the first channel and any past the
    /// second, the right to the second; a mono output takes both sides'
    /// average (Approximation). A sound that ends stops unless it loops.
    /// Nothing is clipped here: Studio hands the device levels past full
    /// scale (seven times it for a sound at Volume 10) and leaves them to
    /// the system, so Rook does too.
    pub fn mix(&mut self, out: &mut [f32], channels: usize) {
        out.fill(0.0);
        if channels == 0 {
            return;
        }
        let frames = out.len() / channels;
        let rate = f64::from(self.rate);
        self.master.clear();
        self.master.resize(frames, [0.0; 2]);
        for bus in self.buses.values_mut() {
            bus.buffer.clear();
            bus.buffer.resize(frames, [0.0; 2]);
        }
        let (scratch, master, buses) = (&mut self.scratch, &mut self.master, &mut self.buses);
        for v in self.voices.values_mut() {
            if v.ended {
                continue;
            }
            scratch.clear();
            scratch.resize(frames, [0.0; 2]);
            v.render(scratch, rate);
            if v.audio.channels <= 1 {
                for s in scratch.iter_mut().flatten() {
                    *s *= MONO_LEVEL;
                }
            }
            let target = match v.group.and_then(|g| buses.get_mut(&g)) {
                Some(bus) => {
                    bus.fed = true;
                    &mut bus.buffer
                }
                None => &mut *master,
            };
            voice::add(scratch, target, v.gains, v.target);
            v.gains = v.target;
        }
        let buses_sounding = self.mix_buses(frames);
        if !self.ambient.is_empty() {
            self.ambient_idle = if !buses_sounding && self.is_empty() {
                self.ambient_idle.saturating_add(frames)
            } else {
                0
            };
            if self.ambient_idle <= self.ambient.tail() {
                self.ambient.process(&mut self.master);
            }
        }
        self.graph.render(&mut self.master);
        for (frame, m) in out.chunks_exact_mut(channels).zip(&self.master) {
            for (c, s) in frame.iter_mut().enumerate() {
                *s = match (channels, c) {
                    (1, _) => (m[0] + m[1]) / 2.0,
                    (_, 1) => m[1],
                    _ => m[0],
                };
            }
        }
    }

    /// Run the buses, innermost first, each into the group it is in (or
    /// the output); whether any is still sounding.
    fn mix_buses(&mut self, frames: usize) -> bool {
        if self.buses.is_empty() {
            return false;
        }
        let depth = |mut k: K| {
            let mut d = 0;
            while let Some(p) = self.buses.get(&k).and_then(|b| b.parent) {
                if d >= MAX_DEPTH || !self.buses.contains_key(&p) {
                    break;
                }
                k = p;
                d += 1;
            }
            d
        };
        let mut order: Vec<(usize, K)> = self.buses.keys().map(|&k| (depth(k), k)).collect();
        order.sort_by_key(|o| std::cmp::Reverse(o.0));
        let mut sounding = false;
        for (_, k) in order {
            let Some(bus) = self.buses.get_mut(&k) else {
                continue;
            };
            bus.idle = if std::mem::take(&mut bus.fed) {
                0
            } else {
                bus.idle.saturating_add(frames)
            };
            if !bus.sounding() {
                continue;
            }
            sounding = true;
            bus.process();
            let buffer = std::mem::take(&mut bus.buffer);
            let into = match bus.parent.and_then(|p| self.buses.get_mut(&p)) {
                Some(parent) => {
                    parent.fed = true;
                    &mut parent.buffer
                }
                None => &mut self.master,
            };
            for (a, b) in buffer.iter().zip(into.iter_mut()) {
                b[0] += a[0];
                b[1] += a[1];
            }
            if let Some(bus) = self.buses.get_mut(&k) {
                bus.buffer = buffer;
            }
        }
        sounding
    }
}

#[cfg(test)]
mod tests;
