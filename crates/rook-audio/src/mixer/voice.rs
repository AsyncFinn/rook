//! One playing sound in the mixer: reading its samples at the output's
//! rate, then its own effects.

use std::sync::Arc;

use rook_content::audio::Audio;

use crate::effects::{Chain, Frame};

#[derive(Debug)]
pub(super) struct Voice<K> {
    pub(super) audio: Arc<Audio>,
    /// Position in source frames.
    pub(super) cursor: f64,
    /// The gains last mixed at, and the ones to reach by the end of the
    /// next buffer: changes ramp over a buffer instead of clicking.
    pub(super) gains: [f32; 2],
    pub(super) target: [f32; 2],
    pub(super) speed: f32,
    pub(super) looped: bool,
    pub(super) generation: u32,
    pub(super) effects: Chain,
    pub(super) group: Option<K>,
}

impl<K> Voice<K> {
    pub(super) fn seconds(&self) -> f64 {
        self.cursor / f64::from(self.audio.sample_rate.max(1))
    }

    /// Fill `out` with the sound at full volume, at `rate` frames per
    /// second, then its effects; false once it has ended (an effect's tail
    /// ends with it, Inference: Studio's sound effects are on the sound's
    /// own channel).
    pub(super) fn render(&mut self, out: &mut [Frame], rate: f64) -> bool {
        let alive = self.read(out, rate);
        if !self.effects.is_empty() {
            self.effects.process(out);
        }
        alive
    }

    fn read(&mut self, out: &mut [Frame], rate: f64) -> bool {
        let audio = &*self.audio;
        let frames = audio.frames();
        let src_channels = usize::from(audio.channels.max(1));
        if frames == 0 {
            return false;
        }
        // Linear interpolation between neighbouring source frames: cheap,
        // and fine for the archive's mostly 8-48 kHz effects
        // (Approximation).
        let step = f64::from(audio.sample_rate) * f64::from(self.speed.max(0.0)) / rate;
        let sample = |frame: usize, ch: usize| f32::from(audio.samples[frame * src_channels + ch]);
        for frame in out.iter_mut() {
            if self.cursor >= frames as f64 {
                if self.looped && step > 0.0 {
                    self.cursor %= frames as f64;
                } else {
                    return false;
                }
            }
            let i = self.cursor as usize;
            let next = if i + 1 < frames {
                i + 1
            } else if self.looped {
                0
            } else {
                i
            };
            let t = (self.cursor - i as f64) as f32;
            for (c, s) in frame.iter_mut().enumerate() {
                // A mono sound goes to both sides.
                let ch = if c < src_channels { c } else { 0 };
                let a = sample(i, ch);
                let b = sample(next, ch);
                *s = (a + (b - a) * t) / 32768.0;
            }
            self.cursor += step;
        }
        true
    }
}

/// Add `from` into `to`, with gains ramping from `gains` to `target`
/// across the buffer.
pub(super) fn add(from: &[Frame], to: &mut [Frame], gains: [f32; 2], target: [f32; 2]) {
    let count = from.len().max(1) as f32;
    for (k, (a, b)) in from.iter().zip(to.iter_mut()).enumerate() {
        let f = k as f32 / count;
        for c in 0..2 {
            b[c] += a[c] * (gains[c] + (target[c] - gains[c]) * f);
        }
    }
}
