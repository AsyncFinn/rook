//! One playing sound: reading its samples at the output's
//! rate, then its own effects.

use std::sync::Arc;

use rook_content::audio::Audio;

use crate::effects::{Chain, Frame};

/// How far a voice's own position may drift from the engine's
/// `TimePosition` before it jumps to the engine's (a script seeking, or a
/// stalled frame). The two run on different clocks, so small drift is
/// normal and jumping on it would click.
const DRIFT_SECONDS: f64 = 0.25;

#[derive(Debug)]
pub(crate) struct Voice<K> {
    pub(crate) audio: Arc<Audio>,
    /// Position in source frames.
    pub(crate) cursor: f64,
    /// The gains last mixed at, and the ones to reach by the end of the
    /// next buffer: changes ramp over a buffer instead of clicking.
    pub(crate) gains: [f32; 2],
    pub(crate) target: [f32; 2],
    pub(crate) speed: f32,
    pub(crate) looped: bool,
    pub(crate) generation: u32,
    pub(crate) effects: Chain,
    pub(crate) group: Option<K>,
}

impl<K> Voice<K> {
    /// A voice starting `position` seconds into `audio`, mixed at `rate`.
    pub(crate) fn new(audio: &Arc<Audio>, position: f64, gains: [f32; 2], rate: u32) -> Self {
        Self {
            audio: audio.clone(),
            cursor: position * f64::from(audio.sample_rate.max(1)),
            gains,
            target: gains,
            speed: 1.0,
            looped: false,
            generation: u32::MAX,
            effects: Chain::new(rate),
            group: None,
        }
    }

    /// Follow the engine: start over at `position` for a new file or a new
    /// `Play` (`generation`), or when it drifted too far from the voice's
    /// own clock.
    pub(crate) fn follow(&mut self, audio: &Arc<Audio>, position: f64, generation: u32) {
        if !Arc::ptr_eq(&self.audio, audio)
            || self.generation != generation
            || (self.seconds() - position).abs() > DRIFT_SECONDS
        {
            self.audio = audio.clone();
            self.cursor = position * f64::from(audio.sample_rate.max(1));
            self.generation = generation;
        }
    }

    pub(crate) fn seconds(&self) -> f64 {
        self.cursor / f64::from(self.audio.sample_rate.max(1))
    }

    /// Fill `out` with the sound at full volume, at `rate` frames per
    /// second, then its effects; false once it has ended (an effect's tail
    /// ends with it, Inference: Studio's sound effects are on the sound's
    /// own channel).
    pub(crate) fn render(&mut self, out: &mut [Frame], rate: f64) -> bool {
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
pub(crate) fn add(from: &[Frame], to: &mut [Frame], gains: [f32; 2], target: [f32; 2]) {
    let count = from.len().max(1) as f32;
    for (k, (a, b)) in from.iter().zip(to.iter_mut()).enumerate() {
        let f = k as f32 / count;
        for c in 0..2 {
            b[c] += a[c] * (gains[c] + (target[c] - gains[c]) * f);
        }
    }
}
