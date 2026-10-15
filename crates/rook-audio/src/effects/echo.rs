//! AudioEcho: the sound again after `DelayTime`, each repeat `Feedback`
//! times the last, with the dry and wet levels in dB (creator-docs).
//!
//! A plain feedback delay (Approximation: with no feedback it matched
//! Studio's level for a recording within 2%, but with feedback Rook's
//! comes out up to 29% louder, `docs/behavior/audio.md`). `RampTime`, how fast a new delay is reached,
//! isn't used: a new delay takes effect at once.

use super::line::Line;
use super::{Frame, db};

/// The longest `DelayTime`, seconds.
const MAX_SECONDS: f32 = 5.0;

#[derive(Debug, Clone)]
pub(super) struct Echo {
    rate: f32,
    delay: f32,
    dry: f32,
    wet: f32,
    feedback: f32,
    line: Line,
}

impl Echo {
    pub(super) fn new(rate: f32, delay: f32, dry: f32, wet: f32, feedback: f32) -> Self {
        let mut e = Self {
            rate,
            delay: 0.0,
            dry: 1.0,
            wet: 1.0,
            feedback: 0.0,
            line: Line::new((rate * MAX_SECONDS) as usize + 1),
        };
        e.set(delay, dry, wet, feedback);
        e
    }

    pub(super) fn set(&mut self, delay: f32, dry: f32, wet: f32, feedback: f32) {
        self.delay = (delay.clamp(0.001, MAX_SECONDS) * self.rate).max(1.0);
        self.dry = db(dry.clamp(-80.0, 10.0));
        self.wet = db(wet.clamp(-80.0, 10.0));
        self.feedback = feedback.clamp(0.0, 1.0);
    }

    /// Frames until the repeats have fallen 60 dB (an hour's worth at
    /// full feedback, which never falls).
    pub(super) fn tail(&self) -> usize {
        let repeats = if self.feedback >= 1.0 {
            1e6
        } else if self.feedback <= 0.0 {
            1.0
        } else {
            (-3.0 / self.feedback.log10()).ceil() + 1.0
        };
        (self.delay * repeats).min(self.rate * 3600.0) as usize
    }

    pub(super) fn process(&mut self, frames: &mut [Frame]) {
        for frame in frames {
            let echo = [0, 1].map(|c| self.line.read(self.delay, c));
            self.line
                .push([0, 1].map(|c| frame[c] + self.feedback * echo[c]));
            for (c, s) in frame.iter_mut().enumerate() {
                *s = *s * self.dry + echo[c] * self.wet;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_with_feedback() {
        let mut e = Echo::new(100.0, 0.1, -80.0, 0.0, 0.5);
        let mut frames = vec![[0.0; 2]; 40];
        frames[0] = [1.0, 1.0];
        e.process(&mut frames);
        let at = |i: usize| (frames[i][0] * 1000.0).round() / 1000.0;
        assert_eq!((at(0), at(10), at(20), at(30)), (0.0, 1.0, 0.5, 0.25));
        assert_eq!(at(5), 0.0);
    }
}
