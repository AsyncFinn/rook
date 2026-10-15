//! SoundGroups in the mixer: each is a bus its sounds (and the groups
//! inside it) are mixed into, which runs its effects, applies its
//! `Volume`, and goes on into the group it is in, or the output.

use crate::effects::{Chain, Frame};

#[derive(Debug)]
pub(super) struct Bus<K> {
    pub(super) parent: Option<K>,
    pub(super) gain: f32,
    pub(super) target: f32,
    pub(super) effects: Chain,
    pub(super) buffer: Vec<Frame>,
    /// Whether the engine still lists the group.
    pub(super) listed: bool,
    /// Whether anything was mixed into it this buffer.
    pub(super) fed: bool,
    /// Frames since anything was mixed into the bus: once past its
    /// effects' tail it is silent, and skipped (or dropped if unlisted).
    pub(super) idle: usize,
}

impl<K> Bus<K> {
    pub(super) fn new(parent: Option<K>, volume: f32, rate: u32) -> Self {
        Self {
            parent,
            gain: volume,
            target: volume,
            effects: Chain::new(rate),
            buffer: Vec::new(),
            listed: true,
            fed: false,
            idle: usize::MAX / 2,
        }
    }

    /// Whether there is anything left to hear from it.
    pub(super) fn sounding(&self) -> bool {
        self.idle <= self.effects.tail()
    }

    /// Run the bus's effects and volume over what was mixed into it.
    pub(super) fn process(&mut self) {
        if !self.effects.is_empty() {
            self.effects.process(&mut self.buffer);
        }
        let count = self.buffer.len().max(1) as f32;
        let (from, to) = (self.gain, self.target);
        self.gain = to;
        for (k, frame) in self.buffer.iter_mut().enumerate() {
            let g = from + (to - from) * (k as f32 / count);
            frame[0] *= g;
            frame[1] *= g;
        }
    }
}
