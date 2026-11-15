//! One effect's stage in a chain: built from its settings, kept when
//! only its settings change, and run over each block.

use super::*;

/// A running effect: its settings and state.
#[derive(Debug, Clone)]
pub(super) enum Stage {
    Equalizer(equalizer::Equalizer),
    Reverb(reverb::State),
    Distortion(f32),
    PitchShift(pitch::Shifter),
    Chorus(chorus::Chorus),
    Echo(echo::Echo),
    Compressor(compressor::Compressor),
    Fader(f32),
}

impl Stage {
    pub(super) fn new(effect: &Effect, rate: f32) -> Self {
        match *effect {
            Effect::Equalizer {
                low,
                mid,
                high,
                range,
            } => Self::Equalizer(equalizer::Equalizer::new(rate, [low, mid, high], range)),
            Effect::Reverb(r) => Self::Reverb(reverb::State::new(rate, r)),
            Effect::Distortion { level } => Self::Distortion(level),
            Effect::PitchShift { octave } => Self::PitchShift(pitch::Shifter::new(rate, octave)),
            Effect::Chorus {
                depth,
                mix,
                rate: r,
            } => Self::Chorus(chorus::Chorus::new(rate, depth, mix, r)),
            Effect::Echo {
                delay,
                dry,
                wet,
                feedback,
            } => Self::Echo(echo::Echo::new(rate, delay, dry, wet, feedback)),
            Effect::Compressor { .. } => Self::Compressor(compressor::Compressor::new(
                rate,
                effect.compressor().expect("a compressor"),
            )),
            Effect::Fader { volume } => Self::Fader(volume),
        }
    }

    /// Take new settings, keeping the state; false if the stage has to be
    /// built again (an equalizer's crossovers moved).
    pub(super) fn set(&mut self, effect: &Effect) -> bool {
        match (self, *effect) {
            (
                Self::Equalizer(e),
                Effect::Equalizer {
                    low,
                    mid,
                    high,
                    range,
                },
            ) => {
                e.set_gains([low, mid, high]);
                return e.crossovers() == range;
            }
            (Self::Reverb(s), Effect::Reverb(r)) => s.set(r),
            (Self::Distortion(l), Effect::Distortion { level }) => *l = level,
            (Self::PitchShift(p), Effect::PitchShift { octave }) => p.set(octave),
            (Self::Chorus(c), Effect::Chorus { depth, mix, rate }) => c.set(depth, mix, rate),
            (
                Self::Echo(e),
                Effect::Echo {
                    delay,
                    dry,
                    wet,
                    feedback,
                },
            ) => e.set(delay, dry, wet, feedback),
            (Self::Compressor(c), e) => match e.compressor() {
                Some(settings) => c.set(settings),
                None => return false,
            },
            (Self::Fader(v), Effect::Fader { volume }) => *v = volume,
            _ => return false,
        }
        true
    }

    pub(super) fn process(&mut self, frames: &mut [Frame], key: Option<&[Frame]>) {
        match self {
            Self::Equalizer(e) => e.process(frames),
            Self::Reverb(s) => s.process(frames),
            Self::Distortion(level) => distortion::process(*level, frames),
            Self::PitchShift(p) => p.process(frames),
            Self::Chorus(c) => c.process(frames),
            Self::Echo(e) => e.process(frames),
            Self::Compressor(c) => c.process(frames, key),
            Self::Fader(v) => {
                let v = v.clamp(0.0, 3.0);
                for s in frames.iter_mut().flatten() {
                    *s *= v;
                }
            }
        }
    }

    /// Whether the stage still sounds after its input stops (a reverb
    /// tail, a delay's buffer), and for how many frames.
    pub(super) fn tail(&self) -> usize {
        match self {
            Self::Reverb(s) => s.tail(),
            Self::PitchShift(p) => p.latency(),
            Self::Chorus(c) => c.latency(),
            Self::Echo(e) => e.tail(),
            _ => 0,
        }
    }
}
