//! PitchShiftSoundEffect: raises or lowers the pitch by `Octave` (a ratio,
//! 0.5 to 2, creator-docs) without changing the speed.
//!
//! Rook reads the input back through a delay line at `Octave` times its
//! speed, with two read points half a window apart, each faded in and out
//! so the jumps back to the write point aren't heard (Approximation:
//! Studio's shifter, FMOD's, works on spectra; the two give the same
//! pitch, not the same artefacts).

use super::Frame;
use super::line::Line;

/// The window: 60 ms, long enough for low voices.
const WINDOW_SECONDS: f32 = 0.06;

/// The level `octave` comes out at: Studio's shifter made a recording
/// louder shifted down (1.04 times at 0.5) and quieter shifted up (0.79 at
/// 2), where Rook's read points lose a little either way. Fitted to those
/// two levels (Approximation).
fn level(octave: f32) -> f32 {
    1.04 * octave.powf(-0.23)
}

#[derive(Debug, Clone)]
pub(super) struct Shifter {
    ratio: f32,
    window: f32,
    line: Line,
    /// How far into the window the first read point is, 0 to 1.
    phase: f32,
}

impl Shifter {
    pub(super) fn new(rate: f32, octave: f32) -> Self {
        let window = (rate * WINDOW_SECONDS).max(4.0);
        Self {
            ratio: octave.clamp(0.5, 2.0),
            window,
            line: Line::new(window as usize + 1),
            phase: 0.0,
        }
    }

    pub(super) fn set(&mut self, octave: f32) {
        self.ratio = octave.clamp(0.5, 2.0);
    }

    pub(super) fn latency(&self) -> usize {
        self.line.len()
    }

    pub(super) fn process(&mut self, frames: &mut [Frame]) {
        if self.ratio == 1.0 {
            return;
        }
        let step = (1.0 - self.ratio) / self.window;
        for frame in frames {
            self.line.push(*frame);
            let second = (self.phase + 0.5) % 1.0;
            // Hann weights half a window apart add up to 1, and each is 0
            // where its read point jumps.
            let weight = |p: f32| 0.5 - 0.5 * (std::f32::consts::TAU * p).cos();
            let (w1, w2) = (weight(self.phase), weight(second));
            let gain = level(self.ratio);
            for (c, s) in frame.iter_mut().enumerate() {
                *s = gain
                    * (w1 * self.line.read(1.0 + self.phase * self.window, c)
                        + w2 * self.line.read(1.0 + second * self.window, c));
            }
            self.phase = (self.phase + step).rem_euclid(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zero crossings per second of a shifted sine: its frequency, twice.
    fn crossings(octave: f32) -> f32 {
        let rate = crate::SAMPLE_RATE as f32;
        let mut p = Shifter::new(rate, octave);
        let mut frames: Vec<Frame> = (0..crate::SAMPLE_RATE)
            .map(|i| [(std::f32::consts::TAU * 300.0 * i as f32 / rate).sin(); 2])
            .collect();
        p.process(&mut frames);
        let tail = &frames[4_800..];
        let n = tail
            .windows(2)
            .filter(|w| (w[0][0] < 0.0) != (w[1][0] < 0.0))
            .count();
        n as f32 / (tail.len() as f32 / rate) / 2.0
    }

    #[test]
    fn shifts_pitch() {
        assert!((crossings(2.0) - 600.0).abs() < 30.0, "{}", crossings(2.0));
        assert!((crossings(0.5) - 150.0).abs() < 15.0, "{}", crossings(0.5));
        assert!((crossings(1.0) - 300.0).abs() < 2.0);
    }
}
