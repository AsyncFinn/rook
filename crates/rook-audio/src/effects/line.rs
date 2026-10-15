//! A stereo delay line read at fractional delays: the pitch shifter's,
//! chorus's and echo's buffer.

use super::Frame;

#[derive(Debug, Clone)]
pub(super) struct Line {
    buffer: Vec<Frame>,
    write: usize,
}

impl Line {
    /// Room for `frames` frames of delay.
    pub(super) fn new(frames: usize) -> Self {
        Self {
            buffer: vec![[0.0; 2]; frames.max(1) + 2],
            write: 0,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Store the newest frame.
    pub(super) fn push(&mut self, frame: Frame) {
        self.buffer[self.write] = frame;
        self.write = (self.write + 1) % self.buffer.len();
    }

    /// The frame pushed `delay` frames before the next push, between
    /// frames by linear interpolation (`delay` 1 is the newest).
    pub(super) fn read(&self, delay: f32, channel: usize) -> f32 {
        let n = self.buffer.len();
        let delay = delay.clamp(0.0, (n - 2) as f32);
        let at = self.write as f32 - delay;
        let at = if at < 0.0 { at + n as f32 } else { at };
        let i = at as usize % n;
        let t = at - at.floor();
        let a = self.buffer[i][channel];
        let b = self.buffer[(i + 1) % n][channel];
        a + (b - a) * t
    }
}
