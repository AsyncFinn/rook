//! Sound effects: the legacy ones (`SoundEffect` subclasses) a Sound or a
//! SoundGroup applies in the order their `Priority` gives, and the audio
//! API's effects (AudioEqualizer, AudioReverb, ...) wired into its graph
//! (`docs/behavior/audio/soundgroups-and-effects.md`).
//!
//! An effect is one DSP for both APIs where Studio's are the same:
//! Equalizer, Reverb, Distortion, PitchShift and Chorus gave the same
//! levels on one recording in either form (Confirmed). A [`Chain`] runs a
//! list of them over stereo frames and keeps their state (filters, reverb
//! tails) from one block to the next.

mod chorus;
mod compressor;
mod distortion;
mod echo;
mod equalizer;
mod line;
mod pitch;
mod reverb;
mod stage;

use stage::Stage;

pub use equalizer::CROSSOVERS;
pub use reverb::{Reverb, ambient};

/// One effect and its settings, as the instance's properties give them
/// (unclamped: each effect keeps to its range when it runs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// Gains in dB for the bands below `range[0]` Hz, between the two and
    /// above `range[1]` ([`CROSSOVERS`] for EqualizerSoundEffect).
    Equalizer {
        low: f32,
        mid: f32,
        high: f32,
        range: [f32; 2],
    },
    Reverb(Reverb),
    /// `Level`, 0 to 1.
    Distortion {
        level: f32,
    },
    /// `Octave` (AudioPitchShifter's `Pitch`): the pitch ratio, 0.5 to 2.
    PitchShift {
        octave: f32,
    },
    /// `Depth` and `Mix` 0 to 1, `Rate` in Hz.
    Chorus {
        depth: f32,
        mix: f32,
        rate: f32,
    },
    /// `DelayTime` in seconds, levels in dB, `Feedback` 0 to 1.
    Echo {
        delay: f32,
        dry: f32,
        wet: f32,
        feedback: f32,
    },
    /// `Threshold` and `MakeupGain` in dB, `Attack` and `Release` in
    /// seconds.
    Compressor {
        threshold: f32,
        ratio: f32,
        attack: f32,
        release: f32,
        makeup: f32,
    },
    /// AudioFader: a plain gain, 0 to 3 (Confirmed: a fader at 2 doubled
    /// a recording's peak exactly).
    Fader {
        volume: f32,
    },
}

impl Effect {
    /// The priority a legacy effect with `Priority` 0 runs at (Confirmed by
    /// bisection against Studio's output: Equalizer and Reverb 8,
    /// Distortion 4, PitchShift 3). The others aren't played as legacy
    /// effects, so they have none.
    pub fn base_priority(&self) -> i32 {
        match self {
            Self::Equalizer { .. } | Self::Reverb(_) => 8,
            Self::Distortion { .. } => 4,
            Self::PitchShift { .. } => 3,
            _ => 0,
        }
    }

    fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    fn compressor(&self) -> Option<compressor::Settings> {
        match *self {
            Self::Compressor {
                threshold,
                ratio,
                attack,
                release,
                makeup,
            } => Some(compressor::Settings {
                threshold,
                ratio,
                attack,
                release,
                makeup,
            }),
            _ => None,
        }
    }
}

/// The order effects run in, from an instance's enabled effects in child
/// order with their `Priority`: higher first, 0 meaning the effect's
/// [base priority](Effect::base_priority) (creator-docs SoundEffect;
/// Confirmed). Of two at the same priority the later child runs first
/// (Inference: three ties in Studio went that way; the docs call the
/// order undetermined).
pub fn ordered(children: impl IntoIterator<Item = (Effect, i32)>) -> Vec<Effect> {
    let mut list: Vec<(i32, usize, Effect)> = children
        .into_iter()
        .enumerate()
        .map(|(i, (e, p))| (if p == 0 { e.base_priority() } else { p }, i, e))
        .collect();
    list.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    list.into_iter().map(|(_, _, e)| e).collect()
}

/// One stereo frame.
pub type Frame = [f32; 2];

#[derive(Debug, Clone)]
pub struct Chain {
    rate: f32,
    effects: Vec<Effect>,
    stages: Vec<Stage>,
}

impl Chain {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate.max(1) as f32,
            effects: Vec::new(),
            stages: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// Run `effects` from now on. Settings change in place; a different
    /// list of kinds starts the chain afresh.
    pub fn sync(&mut self, effects: &[Effect]) {
        if self.effects == effects {
            return;
        }
        let same = self.effects.len() == effects.len()
            && self
                .effects
                .iter()
                .zip(effects)
                .all(|(a, b)| a.same_kind(b));
        if same {
            for (stage, e) in self.stages.iter_mut().zip(effects) {
                if !stage.set(e) {
                    *stage = Stage::new(e, self.rate);
                }
            }
        } else {
            self.stages = effects.iter().map(|e| Stage::new(e, self.rate)).collect();
        }
        self.effects = effects.to_vec();
    }

    pub fn process(&mut self, frames: &mut [Frame]) {
        self.process_keyed(frames, None);
    }

    /// Run the chain, `key` being what a compressor follows (its
    /// sidechain) when given.
    pub fn process_keyed(&mut self, frames: &mut [Frame], key: Option<&[Frame]>) {
        for stage in &mut self.stages {
            stage.process(frames, key);
        }
    }

    /// How many frames the chain goes on sounding after its input stops.
    pub fn tail(&self) -> usize {
        self.stages.iter().map(Stage::tail).sum()
    }
}

pub(crate) fn db(gain: f32) -> f32 {
    10f32.powf(gain / 20.0)
}

#[cfg(test)]
mod tests;
