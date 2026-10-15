//! An AudioAnalyzer's measurements of what reaches it: `RmsLevel`,
//! `PeakLevel` and `GetSpectrum`.
//!
//! Studio measures "the last audio buffer" (creator-docs). Rook measures
//! the last [`BLOCK`] frames (Inference: the length of Studio's buffer
//! isn't known). Both levels are linear: a full-scale peak is 1.
//! `RmsLevel` reads exactly `Sound.PlaybackLoudness` / 1000 for the same
//! stream in Studio (Confirmed), and Rook takes it over both sides of its
//! stereo stream. Studio averages over its stream's own channels, so a
//! stream it has widened (an AudioListener's output, an AudioEcho's or
//! AudioCompressor's) reads lower there than here (Approximation).

use crate::effects::Frame;

/// Frames the levels are taken over.
pub const BLOCK: usize = 1024;
/// Frames kept: the largest spectrum window.
const HISTORY: usize = 4096;

#[derive(Debug, Clone)]
pub struct Meter {
    history: Vec<Frame>,
    /// Where the next frame goes.
    at: usize,
}

impl Default for Meter {
    fn default() -> Self {
        Self {
            history: vec![[0.0; 2]; HISTORY],
            at: 0,
        }
    }
}

impl Meter {
    pub fn feed(&mut self, frames: &[Frame]) {
        for f in frames {
            self.history[self.at] = *f;
            self.at = (self.at + 1) % HISTORY;
        }
    }

    /// The last `n` frames, oldest first.
    fn last(&self, n: usize) -> impl Iterator<Item = &Frame> {
        let n = n.min(HISTORY);
        (0..n).map(move |k| &self.history[(self.at + HISTORY - n + k) % HISTORY])
    }

    /// `RmsLevel` and `PeakLevel`: the root mean square and the largest
    /// sample of the last block, over both sides.
    pub fn levels(&self) -> (f32, f32) {
        let mut sum = 0.0f64;
        let mut peak = 0.0f32;
        for f in self.last(BLOCK) {
            for s in f {
                sum += f64::from(*s) * f64::from(*s);
                peak = peak.max(s.abs());
            }
        }
        (((sum / (2 * BLOCK) as f64).sqrt()) as f32, peak)
    }

    /// `GetSpectrum` with `bins` bins (128, 512 or 2048 for
    /// `AudioWindowSize` Small, Medium and Large, Confirmed): root mean
    /// square levels evenly spaced from 0 Hz to half of `rate`, bin `k` at
    /// `k * rate / 2 / bins` Hz (creator-docs gives 0 to 24,000 Hz).
    /// Rook takes a Hann-windowed transform of the last `2 * bins` frames,
    /// scaled so the bins' squares add up to the window's mean square
    /// (Inference: Studio's added up to 1.05 to 1.1 times it for a
    /// recording).
    pub fn spectrum(&self, bins: usize) -> Vec<f32> {
        let n = (2 * bins).clamp(2, HISTORY).next_power_of_two();
        let window = |k: usize| 0.5 - 0.5 * (std::f32::consts::TAU * k as f32 / n as f32).cos();
        let mut power = vec![0.0f32; n / 2];
        for c in 0..2 {
            let mut re: Vec<f32> = self
                .last(n)
                .enumerate()
                .map(|(k, f)| f[c] * window(k))
                .collect();
            let mut im = vec![0.0; n];
            fft(&mut re, &mut im);
            for (k, p) in power.iter_mut().enumerate() {
                *p += (re[k] * re[k] + im[k] * im[k]) / 2.0;
            }
        }
        // A Hann window keeps 3/8 of the power; one side of the transform
        // holds half of it, n times over.
        let scale = 2.0 / (0.375 * (n * n) as f32);
        power.iter().map(|p| (p * scale).sqrt()).collect()
    }
}

/// In-place radix-2 transform; `re.len()` is a power of two.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -std::f32::consts::TAU / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (angle * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let (xr, xi) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_spectrum_of_a_sine() {
        let rate = 48_000.0;
        let mut m = Meter::default();
        // 1500 Hz, amplitude 0.5: bin 32 of 512 (46.875 Hz apart).
        let frames: Vec<Frame> = (0..HISTORY)
            .map(|i| [0.5 * (std::f32::consts::TAU * 1500.0 * i as f32 / rate).sin(); 2])
            .collect();
        m.feed(&frames);
        let (rms, peak) = m.levels();
        assert!((rms - 0.3536).abs() < 0.002 && (peak - 0.5).abs() < 0.002);
        let s = m.spectrum(512);
        assert_eq!(s.len(), 512);
        let top = (0..512).max_by(|&a, &b| s[a].total_cmp(&s[b])).unwrap();
        assert_eq!(top, 32);
        let total = s.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((total - rms).abs() < 0.01, "{total}");
    }
}
