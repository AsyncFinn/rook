//! AudioCompressor: turns the sound down by `Ratio` above `Threshold`,
//! following its level in `Attack` and `Release` seconds, then adds
//! `MakeupGain` dB (creator-docs). The level it follows is its
//! **Sidechain** input's when one is wired, else its own.
//!
//! A feed-forward compressor following the level of both sides together,
//! `sqrt(L² + R²)` (Approximation: of the detectors tried, the one that
//! brought Rook within 8% of Studio's levels for a recording at five
//! settings; at the defaults Rook is 0.6 times as loud. `MakeupGain` was
//! exact; `docs/behavior/audio/audio-api.md`).

use super::{Frame, db};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Settings {
    pub(super) threshold: f32,
    pub(super) ratio: f32,
    pub(super) attack: f32,
    pub(super) release: f32,
    pub(super) makeup: f32,
}

#[derive(Debug, Clone)]
pub(super) struct Compressor {
    rate: f32,
    settings: Settings,
    /// The level followed, linear.
    envelope: f32,
}

impl Compressor {
    pub(super) fn new(rate: f32, settings: Settings) -> Self {
        Self {
            rate,
            settings,
            envelope: 0.0,
        }
    }

    pub(super) fn set(&mut self, settings: Settings) {
        self.settings = settings;
    }

    /// Compress `frames`, following `key`'s level (the sidechain) when
    /// given, else their own.
    pub(super) fn process(&mut self, frames: &mut [Frame], key: Option<&[Frame]>) {
        let s = self.settings;
        let coefficient = |seconds: f32| 1.0 - (-1.0 / (seconds.max(1e-4) * self.rate)).exp();
        let attack = coefficient(s.attack.clamp(0.0001, 0.5));
        let release = coefficient(s.release.clamp(0.01, 5.0));
        let threshold = s.threshold.clamp(-60.0, 0.0);
        let slope = 1.0 - 1.0 / s.ratio.clamp(1.0, 50.0);
        let makeup = s.makeup.clamp(-30.0, 30.0);
        for (i, frame) in frames.iter_mut().enumerate() {
            let k = key.map_or(*frame, |k| k.get(i).copied().unwrap_or([0.0; 2]));
            let level = (k[0] * k[0] + k[1] * k[1]).sqrt();
            let a = if level > self.envelope {
                attack
            } else {
                release
            };
            self.envelope += a * (level - self.envelope);
            let over = 20.0 * self.envelope.max(1e-9).log10() - threshold;
            let gain = db(makeup - over.max(0.0) * slope);
            frame[0] *= gain;
            frame[1] *= gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_loud_sound_down() {
        let settings = Settings {
            threshold: -20.0,
            ratio: 4.0,
            attack: 0.0001,
            release: 0.1,
            makeup: 0.0,
        };
        let mut c = Compressor::new(1000.0, settings);
        let mut frames = vec![[1.0, 1.0]; 1000];
        c.process(&mut frames, None);
        // Both sides at full scale read +3 dB: 23 dB over the threshold,
        // at 4:1, comes out 5.75 dB over it.
        let out = 20.0 * frames[999][0].log10();
        assert!((out + 17.26).abs() < 0.1, "{out}");
        let mut quiet = vec![[0.01, 0.01]; 10];
        c.process(&mut quiet, Some(&[[0.0; 2]; 10]));
        assert!(quiet[9][0] < 0.01);
    }
}
