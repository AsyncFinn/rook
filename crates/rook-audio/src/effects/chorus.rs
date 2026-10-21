//! AudioChorus and ChorusSoundEffect: copies of the sound, each delayed by
//! a slowly swinging amount, mixed back in. Studio's two are the same
//! effect: on one recording they gave the same level at every setting
//! tried (Confirmed, `docs/behavior/audio/audio-api.md`).
//!
//! Rook's chorus has three copies whose delays swing around 20 ms by up
//! to `Depth` times 10 ms at `Rate` Hz, a third of a cycle apart, the
//! right side a quarter cycle on from the left; `Mix` is the share of the
//! copies in the output (Approximation: the shape of Studio's chorus,
//! FMOD's, isn't measured, only its level, which the copies' gain is
//! fitted to).

use super::Frame;
use super::line::Line;

const BASE_SECONDS: f32 = 0.020;
const SWING_SECONDS: f32 = 0.010;
const VOICES: usize = 3;
/// The copies' gain, fitted to Studio: a recording came out 1.04 times as
/// loud with the defaults and 1.0 times with `Mix` and `Depth` 1, where
/// Rook gives 0.93 and 1.06.
const WET: f32 = 1.9;

#[derive(Debug, Clone)]
pub(super) struct Chorus {
    rate: f32,
    depth: f32,
    mix: f32,
    speed: f32,
    phase: f32,
    line: Line,
}

impl Chorus {
    pub(super) fn new(rate: f32, depth: f32, mix: f32, speed: f32) -> Self {
        let mut c = Self {
            rate,
            depth: 0.0,
            mix: 0.0,
            speed: 0.0,
            phase: 0.0,
            line: Line::new((rate * (BASE_SECONDS + SWING_SECONDS)) as usize + 2),
        };
        c.set(depth, mix, speed);
        c
    }

    pub(super) fn set(&mut self, depth: f32, mix: f32, speed: f32) {
        self.depth = depth.clamp(0.0, 1.0);
        self.mix = mix.clamp(0.0, 1.0);
        self.speed = speed.clamp(0.0, 20.0);
    }

    pub(super) fn latency(&self) -> usize {
        self.line.len()
    }

    pub(super) fn process(&mut self, frames: &mut [Frame]) {
        let base = BASE_SECONDS * self.rate;
        let swing = SWING_SECONDS * self.rate * self.depth;
        let step = self.speed / self.rate;
        for frame in frames {
            self.line.push(*frame);
            for (c, s) in frame.iter_mut().enumerate() {
                let mut wet = 0.0;
                for v in 0..VOICES {
                    let p = self.phase + v as f32 / VOICES as f32 + c as f32 * 0.25;
                    let delay = base + swing * (std::f32::consts::TAU * p).sin();
                    wet += self.line.read(1.0 + delay, c);
                }
                let wet = wet / VOICES as f32 * WET;
                *s = *s * (1.0 - self.mix) + wet * self.mix;
            }
            self.phase = (self.phase + step).rem_euclid(1.0);
        }
    }
}
