//! ReverbSoundEffect, and `SoundService.AmbientReverb`'s presets.
//!
//! Rook's reverb is a feedback delay network: eight delay lines mixed into
//! each other every pass, each losing what makes the tail fall by 60 dB in
//! `DecayTime`, after a few all-pass filters that smear the input
//! (`Diffusion`). `Density` spaces the lines out. This is an
//! Approximation: Studio's is FMOD's SFX reverb, with early reflections
//! and high-frequency decay of its own. The wet level is scaled so the
//! levels Studio gave a recording at three decay times are matched
//! (`docs/behavior/audio/soundgroups-and-effects.md`).

use super::{Frame, db};

/// One reverb's settings: ReverbSoundEffect's properties, or a preset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reverb {
    /// Seconds for the tail to fall 60 dB, 0.1 to 20.
    pub decay: f32,
    /// 0 to 1.
    pub density: f32,
    /// 0 to 1.
    pub diffusion: f32,
    /// dB, -80 to 20 (AudioReverb; the engine keeps ReverbSoundEffect to
    /// its -80 to 10).
    pub dry: f32,
    /// dB, as `dry`.
    pub wet: f32,
    /// Hz above which the tail is cut (20 kHz for ReverbSoundEffect).
    pub high_cut: f32,
}

impl Default for Reverb {
    /// ReverbSoundEffect's defaults (Confirmed in Studio).
    fn default() -> Self {
        Self {
            decay: 1.5,
            density: 1.0,
            diffusion: 1.0,
            dry: -6.0,
            wet: 0.0,
            high_cut: 20_000.0,
        }
    }
}

/// What `SoundService.AmbientReverb` (an `Enum.ReverbType` value) adds to
/// every sound: `None` for NoReverb. Approximation: the enum lists FMOD's
/// reverb presets by name and in FMOD's order, so Rook uses those presets'
/// decay, diffusion, density, wet level and high cut, as FMOD publishes
/// them, with the wet level lowered by `AMBIENT_WET_DB` to match a
/// recording of Studio. The dry sound passes unchanged.
pub fn ambient(reverb_type: u32) -> Option<Reverb> {
    // (decay ms, diffusion %, density %, high cut Hz, wet dB)
    const PRESETS: [(f32, f32, f32, f32, f32); 23] = [
        (1500.0, 100.0, 100.0, 14_500.0, -8.0),  // GenericReverb
        (170.0, 100.0, 100.0, 160.0, -7.8),      // PaddedCell
        (400.0, 100.0, 100.0, 6_050.0, -9.4),    // Room
        (1500.0, 100.0, 60.0, 2_900.0, 0.5),     // Bathroom
        (500.0, 100.0, 100.0, 160.0, -19.0),     // LivingRoom
        (2300.0, 100.0, 100.0, 7_800.0, -8.5),   // StoneRoom
        (4300.0, 100.0, 100.0, 5_850.0, -11.7),  // Auditorium
        (3900.0, 100.0, 100.0, 5_650.0, -9.8),   // ConcertHall
        (2900.0, 100.0, 100.0, 20_000.0, -11.3), // Cave
        (7200.0, 100.0, 100.0, 4_500.0, -9.6),   // Arena
        (10000.0, 100.0, 100.0, 3_400.0, -7.4),  // Hangar
        (300.0, 100.0, 100.0, 500.0, -24.0),     // CarpettedHallway
        (1500.0, 100.0, 100.0, 7_800.0, -5.5),   // Hallway
        (270.0, 100.0, 100.0, 9_000.0, -6.0),    // StoneCorridor
        (1500.0, 100.0, 100.0, 8_300.0, -9.8),   // Alley
        (1500.0, 79.0, 100.0, 760.0, -12.3),     // Forest
        (1500.0, 50.0, 100.0, 4_050.0, -26.0),   // City
        (1500.0, 27.0, 100.0, 1_220.0, -24.0),   // Mountains
        (1500.0, 100.0, 100.0, 3_400.0, -5.0),   // Quarry
        (1500.0, 21.0, 100.0, 1_670.0, -28.0),   // Plain
        (1700.0, 100.0, 100.0, 20_000.0, -19.5), // ParkingLot
        (2800.0, 80.0, 60.0, 3_400.0, 1.2),      // SewerPipe
        (1500.0, 100.0, 100.0, 500.0, 7.0),      // UnderWater
    ];
    let &(decay, diffusion, density, high_cut, wet) =
        PRESETS.get(usize::try_from(reverb_type).ok()?.checked_sub(1)?)?;
    Some(Reverb {
        decay: decay / 1000.0,
        density: density / 100.0,
        diffusion: diffusion / 100.0,
        dry: 0.0,
        wet: wet + AMBIENT_WET_DB,
        high_cut,
    })
}

/// How much quieter Studio's ambient reverb is than FMOD's preset wet level
/// gives through Rook's reverb: recorded after a short tone, Studio's tails
/// for Cave, Hallway and ConcertHall were 15.9, 17.0 and 14.5 dB below
/// Rook's without this (`docs/behavior/audio/soundgroups-and-effects.md`).
const AMBIENT_WET_DB: f32 = -16.0;

/// The delay lines' lengths at full density, in ms: spread out and with
/// no common factors, so their echoes don't line up.
const LINES_MS: [f32; 8] = [29.7, 37.1, 41.1, 43.7, 53.9, 59.3, 67.1, 73.3];
/// The input all-passes' lengths, ms.
const DIFFUSERS_MS: [f32; 4] = [4.77, 3.59, 12.73, 9.31];
/// The wet level is `WET_SCALE / sqrt(WET_DECAY + decay)`: Studio's tail
/// gets only a little louder with a longer decay (wet only, a recording
/// read 1.25, 1.37 and 1.50 times the dry level at 0.3, 1.5 and 5 s),
/// where a plain network's energy grows with its length. Fitted to those
/// three levels.
const WET_SCALE: f32 = 0.6;
const WET_DECAY: f32 = 0.84;

#[derive(Debug, Clone)]
struct Delay {
    buffer: Vec<f32>,
    at: usize,
}

impl Delay {
    fn new(frames: usize) -> Self {
        Self {
            buffer: vec![0.0; frames.max(1)],
            at: 0,
        }
    }

    fn out(&self) -> f32 {
        self.buffer[self.at]
    }

    fn push(&mut self, x: f32) {
        self.buffer[self.at] = x;
        self.at = (self.at + 1) % self.buffer.len();
    }
}

#[derive(Debug, Clone)]
pub(crate) struct State {
    rate: f32,
    settings: Reverb,
    lines: Vec<Delay>,
    /// Each line's gain per pass, from the decay time and its length.
    feedback: [f32; 8],
    damping: [f32; 8],
    /// Each line's loop low-pass state.
    low: [f32; 8],
    diffusers: Vec<Delay>,
    /// The high cut's one-pole coefficient and state per side.
    cut: f32,
    cut_state: [f32; 2],
}

impl State {
    pub(crate) fn new(rate: f32, settings: Reverb) -> Self {
        let mut s = Self {
            rate,
            settings,
            lines: Vec::new(),
            feedback: [0.0; 8],
            damping: [0.0; 8],
            low: [0.0; 8],
            diffusers: DIFFUSERS_MS
                .iter()
                .map(|ms| Delay::new((ms * rate / 1000.0) as usize))
                .collect(),
            cut: 1.0,
            cut_state: [0.0; 2],
        };
        s.build();
        s
    }

    pub(crate) fn set(&mut self, settings: Reverb) {
        let respace = settings.density != self.settings.density;
        self.settings = settings;
        if respace {
            self.build();
        } else {
            self.tune();
        }
    }

    fn build(&mut self) {
        // Less density, further apart (up to twice as far at 0).
        let spread = 2.0 - self.settings.density.clamp(0.0, 1.0);
        self.lines = LINES_MS
            .iter()
            .map(|ms| Delay::new((ms * spread * self.rate / 1000.0) as usize))
            .collect();
        self.tune();
    }

    fn tune(&mut self) {
        let decay = self.settings.decay.clamp(0.1, 20.0);
        for (i, line) in self.lines.iter().enumerate() {
            let seconds = line.buffer.len() as f32 / self.rate;
            self.feedback[i] = 10f32.powf(-3.0 * seconds / decay);
        }
        // Highs fade faster than lows in a room: a gentle loop low-pass.
        self.damping = [0.2; 8];
        let cut = self.settings.high_cut.clamp(20.0, self.rate / 2.0);
        self.cut = 1.0 - (-std::f32::consts::TAU * cut / self.rate).exp();
    }

    /// Frames the tail lasts once the input stops.
    pub(crate) fn tail(&self) -> usize {
        (self.settings.decay.clamp(0.1, 20.0) * self.rate) as usize
    }

    pub(crate) fn process(&mut self, frames: &mut [Frame]) {
        let dry = db(self.settings.dry.clamp(-80.0, 20.0));
        let decay = self.settings.decay.clamp(0.1, 20.0);
        let wet = db(self.settings.wet.clamp(-80.0, 20.0)) * WET_SCALE / (WET_DECAY + decay).sqrt();
        let g = 0.7 * self.settings.diffusion.clamp(0.0, 1.0);
        let mut taps = [0.0f32; 8];
        for frame in frames {
            let mut x = (frame[0] + frame[1]) * 0.5;
            for d in &mut self.diffusers {
                // Schroeder all-pass: flat in level, smeared in time.
                let delayed = d.out();
                let v = x + g * delayed;
                d.push(v);
                x = delayed - g * v;
            }
            for (t, line) in taps.iter_mut().zip(&self.lines) {
                *t = line.out();
            }
            // Householder mixing: every line feeds every other, keeping the
            // energy.
            let sum: f32 = taps.iter().sum::<f32>() * (2.0 / 8.0);
            for (i, line) in self.lines.iter_mut().enumerate() {
                let fed = (taps[i] - sum) * self.feedback[i];
                let low = &mut self.low[i];
                *low = fed + self.damping[i] * (*low - fed);
                line.push(x + *low);
            }
            let left = taps[0] - taps[2] + taps[4] - taps[6];
            let right = taps[1] - taps[3] + taps[5] - taps[7];
            for (c, out) in [left, right].into_iter().enumerate() {
                let state = &mut self.cut_state[c];
                *state += self.cut * (out - *state);
                frame[c] = frame[c] * dry + *state * wet;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tail of an impulse falls by about 60 dB in the decay time.
    #[test]
    fn decays_in_time() {
        let rate = 8_000.0;
        let r = Reverb {
            dry: -80.0,
            ..Reverb::default()
        };
        let mut s = State::new(rate, r);
        let mut frames = vec![[0.0f32; 2]; 8_000 * 3];
        frames[0] = [1.0, 1.0];
        s.process(&mut frames);
        let energy = |from: f32, to: f32| -> f32 {
            frames[(from * rate) as usize..(to * rate) as usize]
                .iter()
                .map(|f| f[0] * f[0])
                .sum::<f32>()
        };
        let fall = 10.0 * (energy(0.2, 0.3) / energy(1.7, 1.8)).log10();
        assert!((fall - 60.0).abs() < 12.0, "fell {fall} dB in 1.5 s");
    }

    #[test]
    fn presets() {
        assert_eq!(ambient(0), None);
        assert_eq!(ambient(9).map(|r| r.decay), Some(2.9));
        assert_eq!(ambient(23).map(|r| r.wet), Some(7.0 + AMBIENT_WET_DB));
        assert_eq!(ambient(24), None);
    }
}
