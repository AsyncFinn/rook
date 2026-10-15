//! The legacy sound effects (`SoundEffect` subclasses) a Sound or a
//! SoundGroup applies to what it plays, in the order their `Priority`
//! gives (`docs/behavior/audio.md`, "Effects").
//!
//! Rook implements the four the archive uses most: EqualizerSoundEffect,
//! ReverbSoundEffect, DistortionSoundEffect and PitchShiftSoundEffect. A
//! [`Chain`] runs a list of them over stereo frames and keeps their state
//! (filters, reverb tails) from one block to the next.

mod distortion;
mod equalizer;
mod pitch;
mod reverb;

pub use reverb::{Reverb, ambient};

/// One effect and its settings, as the instance's properties give them
/// (unclamped: each effect keeps to its range when it runs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// Gains in dB for the bands below 400 Hz, 400 to 4000 Hz and above.
    Equalizer {
        low: f32,
        mid: f32,
        high: f32,
    },
    Reverb(Reverb),
    /// `Level`, 0 to 1.
    Distortion {
        level: f32,
    },
    /// `Octave`: the pitch ratio, 0.5 to 2.
    PitchShift {
        octave: f32,
    },
}

impl Effect {
    /// The priority an effect with `Priority` 0 runs at (Confirmed by
    /// bisection against Studio's output: Equalizer and Reverb 8,
    /// Distortion 4, PitchShift 3).
    pub fn base_priority(&self) -> i32 {
        match self {
            Self::Equalizer { .. } | Self::Reverb(_) => 8,
            Self::Distortion { .. } => 4,
            Self::PitchShift { .. } => 3,
        }
    }

    fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
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

/// A running effect: its settings and state.
#[derive(Debug, Clone)]
enum Stage {
    Equalizer(equalizer::Equalizer),
    Reverb(reverb::State),
    Distortion(f32),
    PitchShift(pitch::Shifter),
}

impl Stage {
    fn new(effect: &Effect, rate: f32) -> Self {
        match *effect {
            Effect::Equalizer { low, mid, high } => {
                Self::Equalizer(equalizer::Equalizer::new(rate, [low, mid, high]))
            }
            Effect::Reverb(r) => Self::Reverb(reverb::State::new(rate, r)),
            Effect::Distortion { level } => Self::Distortion(level),
            Effect::PitchShift { octave } => Self::PitchShift(pitch::Shifter::new(rate, octave)),
        }
    }

    /// Take new settings, keeping the state.
    fn set(&mut self, effect: &Effect) {
        match (self, *effect) {
            (Self::Equalizer(e), Effect::Equalizer { low, mid, high }) => {
                e.set_gains([low, mid, high])
            }
            (Self::Reverb(s), Effect::Reverb(r)) => s.set(r),
            (Self::Distortion(l), Effect::Distortion { level }) => *l = level,
            (Self::PitchShift(p), Effect::PitchShift { octave }) => p.set(octave),
            _ => {}
        }
    }

    fn process(&mut self, frames: &mut [Frame]) {
        match self {
            Self::Equalizer(e) => e.process(frames),
            Self::Reverb(s) => s.process(frames),
            Self::Distortion(level) => distortion::process(*level, frames),
            Self::PitchShift(p) => p.process(frames),
        }
    }

    /// Whether the stage still sounds after its input stops (a reverb
    /// tail, a pitch shifter's buffer), and for how many frames.
    fn tail(&self) -> usize {
        match self {
            Self::Reverb(s) => s.tail(),
            Self::PitchShift(p) => p.latency(),
            _ => 0,
        }
    }
}

/// Effects run one after another over stereo frames at one rate.
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
                stage.set(e);
            }
        } else {
            self.stages = effects.iter().map(|e| Stage::new(e, self.rate)).collect();
        }
        self.effects = effects.to_vec();
    }

    pub fn process(&mut self, frames: &mut [Frame]) {
        for stage in &mut self.stages {
            stage.process(frames);
        }
    }

    /// How many frames the chain goes on sounding after its input stops.
    pub fn tail(&self) -> usize {
        self.stages.iter().map(Stage::tail).sum()
    }
}

/// From decibels to a linear gain.
pub(crate) fn db(gain: f32) -> f32 {
    10f32.powf(gain / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EQ: Effect = Effect::Equalizer {
        low: 0.0,
        mid: 0.0,
        high: 0.0,
    };
    const DIST: Effect = Effect::Distortion { level: 0.5 };
    const PITCH: Effect = Effect::PitchShift { octave: 2.0 };

    /// The orders measured in Studio: at Priority 0 Equalizer before
    /// Distortion before PitchShift; an explicit priority above or below
    /// the base moves an effect; ties go to the later child.
    #[test]
    fn orders_by_priority() {
        assert_eq!(ordered([(PITCH, 0), (DIST, 0), (EQ, 0)]), [EQ, DIST, PITCH]);
        assert_eq!(ordered([(DIST, 9), (EQ, 0)]), [DIST, EQ]);
        assert_eq!(ordered([(DIST, 8), (EQ, 0)]), [EQ, DIST]);
        assert_eq!(ordered([(DIST, 0), (EQ, 3)]), [DIST, EQ]);
        assert_eq!(ordered([(DIST, 0), (EQ, 4)]), [EQ, DIST]);
        assert_eq!(ordered([(PITCH, 0), (DIST, 2)]), [PITCH, DIST]);
        assert_eq!(ordered([(DIST, -5), (EQ, -10)]), [DIST, EQ]);
    }

    #[test]
    fn keeps_state_across_settings() {
        let mut chain = Chain::new(48_000);
        chain.sync(&[EQ]);
        let mut frames = vec![[0.5, 0.5]; 64];
        chain.process(&mut frames);
        chain.sync(&[Effect::Equalizer {
            low: -6.0,
            mid: 0.0,
            high: 0.0,
        }]);
        assert!(matches!(chain.stages[0], Stage::Equalizer(_)));
        chain.sync(&[DIST]);
        assert!(matches!(chain.stages[0], Stage::Distortion(_)));
    }
}
