//! EqualizerSoundEffect and AudioEqualizer: three bands, below 400 Hz,
//! 400 to 4000 Hz and above (creator-docs; AudioEqualizer's `MidRange`
//! moves the two crossovers), each with a gain of -80 to 10 dB. Studio
//! keeps to that range when it plays even though the legacy property reads
//! back what was set: +30 dB on every band was exactly as loud as +10
//! (Confirmed). The two classes are one effect: they gave the same levels
//! on one recording (Confirmed).
//!
//! The bands are split by Linkwitz-Riley crossovers of 24 dB per octave,
//! whose bands add back up to the input (Inference: FMOD's three-band EQ
//! works so, with the same bands, and the levels Studio gave a recording
//! with each band cut match within a few percent).

use super::{Frame, db};

/// EqualizerSoundEffect's crossovers, and AudioEqualizer's default
/// `MidRange`.
pub const CROSSOVERS: [f32; 2] = [400.0, 4000.0];

/// A second-order filter (the Audio EQ Cookbook's forms).
#[derive(Debug, Clone, Copy, Default)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    /// Per channel: the last two inputs and outputs, as transposed direct
    /// form II state.
    z: [[f32; 2]; 2],
}

/// Which way a [`Biquad`] passes.
#[derive(Clone, Copy)]
enum Pass {
    Low,
    High,
    All,
}

impl Biquad {
    /// Butterworth (Q 1/√2) at `frequency`.
    fn new(kind: Pass, frequency: f32, rate: f32) -> Self {
        let w = std::f32::consts::TAU * (frequency / rate).min(0.49);
        let (sin, cos) = w.sin_cos();
        let alpha = sin / std::f32::consts::SQRT_2;
        let a0 = 1.0 + alpha;
        let b = match kind {
            Pass::Low => [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0],
            Pass::High => [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0],
            Pass::All => [1.0 - alpha, -2.0 * cos, 1.0 + alpha],
        };
        Self {
            b: b.map(|v| v / a0),
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            z: [[0.0; 2]; 2],
        }
    }

    fn run(&mut self, channel: usize, x: f32) -> f32 {
        let z = &mut self.z[channel];
        let y = self.b[0] * x + z[0];
        z[0] = self.b[1] * x - self.a[0] * y + z[1];
        z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// A Linkwitz-Riley filter: two Butterworth sections in a row.
#[derive(Debug, Clone, Copy)]
struct Lr4([Biquad; 2]);

impl Lr4 {
    fn new(low_pass: bool, frequency: f32, rate: f32) -> Self {
        let one = || {
            let kind = if low_pass { Pass::Low } else { Pass::High };
            Biquad::new(kind, frequency, rate)
        };
        Self([one(), one()])
    }

    fn run(&mut self, channel: usize, x: f32) -> f32 {
        let y = self.0[0].run(channel, x);
        self.0[1].run(channel, y)
    }
}

#[derive(Debug, Clone)]
pub(super) struct Equalizer {
    low_pass: Lr4,
    low_high_pass: Lr4,
    mid_pass: Lr4,
    high_pass: Lr4,
    /// Gives the low band the phase the high crossover gives the others,
    /// so the three add back up flat.
    low_phase: Biquad,
    gains: [f32; 3],
    crossovers: [f32; 2],
}

impl Equalizer {
    pub(super) fn new(rate: f32, gains: [f32; 3], crossovers: [f32; 2]) -> Self {
        let [low, high] = crossovers;
        let mut e = Self {
            low_pass: Lr4::new(true, low, rate),
            low_high_pass: Lr4::new(false, low, rate),
            mid_pass: Lr4::new(true, high, rate),
            high_pass: Lr4::new(false, high, rate),
            low_phase: Biquad::new(Pass::All, high, rate),
            gains: [1.0; 3],
            crossovers,
        };
        e.set_gains(gains);
        e
    }

    pub(super) fn crossovers(&self) -> [f32; 2] {
        self.crossovers
    }

    pub(super) fn set_gains(&mut self, gains: [f32; 3]) {
        self.gains = gains.map(|g| db(g.clamp(-80.0, 10.0)));
    }

    pub(super) fn process(&mut self, frames: &mut [Frame]) {
        let [low, mid, high] = self.gains;
        for frame in frames {
            for (c, s) in frame.iter_mut().enumerate() {
                let x = *s;
                let l = self.low_phase.run(c, self.low_pass.run(c, x));
                let rest = self.low_high_pass.run(c, x);
                let m = self.mid_pass.run(c, rest);
                let h = self.high_pass.run(c, rest);
                *s = l * low + m * mid + h * high;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine's level through the equalizer, once it has settled.
    fn level(frequency: f32, gains: [f32; 3]) -> f32 {
        let rate = crate::SAMPLE_RATE as f32;
        let mut eq = Equalizer::new(rate, gains, CROSSOVERS);
        let mut frames: Vec<Frame> = (0..crate::SAMPLE_RATE)
            .map(|i| [(std::f32::consts::TAU * frequency * i as f32 / rate).sin(); 2])
            .collect();
        eq.process(&mut frames);
        let tail = &frames[24_000..];
        (tail.iter().map(|f| f[0] * f[0]).sum::<f32>() / tail.len() as f32 * 2.0).sqrt()
    }

    #[test]
    fn bands() {
        // Flat at 0 dB everywhere, crossovers included.
        for f in [50.0, 400.0, 1000.0, 4000.0, 10_000.0] {
            assert!((level(f, [0.0; 3]) - 1.0).abs() < 0.01, "{f} Hz");
        }
        // Each band's gain where it is, the others' far from it.
        assert!((level(100.0, [-20.0, 0.0, 0.0]) - 0.1).abs() < 0.01);
        assert!((level(1500.0, [0.0, 10.0, 0.0]) - 3.0).abs() < 0.2);
        assert!(level(12_000.0, [0.0, 0.0, -80.0]) < 0.02);
        // Studio's range: +30 dB plays as +10.
        assert_eq!(
            level(1500.0, [0.0, 30.0, 0.0]),
            level(1500.0, [0.0, 10.0, 0.0])
        );
    }
}
