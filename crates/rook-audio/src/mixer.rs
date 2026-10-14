//! Mixing playing sounds into one stream.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use rook_content::audio::Audio;

/// How far the mixer's own position may drift from the engine's
/// `TimePosition` before it jumps to the engine's (a script seeking, or a
/// stalled frame). The two run on different clocks, so small drift is
/// normal and jumping on it would click.
const DRIFT_SECONDS: f64 = 0.25;

/// One sound to hear, as the engine sees it this frame.
#[derive(Debug, Clone)]
pub struct Play<K> {
    pub key: K,
    pub audio: Arc<Audio>,
    /// Seconds into the sound.
    pub position: f64,
    /// Linear gain for the left and right of the output: `Volume`, with a
    /// 3D sound's roll-off and panning ([`crate::spatial`]).
    pub gains: [f32; 2],
    /// 1 is the sound's own rate.
    pub speed: f32,
    pub looped: bool,
    /// Changes when the sound is played again: the mixer then starts it
    /// over at `position`, however close it already was.
    pub generation: u32,
}

#[derive(Debug)]
struct Voice {
    audio: Arc<Audio>,
    /// Position in source frames.
    cursor: f64,
    /// The gains last mixed at, and the ones to reach by the end of the
    /// next buffer: changes ramp over a buffer instead of clicking.
    gains: [f32; 2],
    target: [f32; 2],
    speed: f32,
    looped: bool,
    generation: u32,
}

impl Voice {
    fn seconds(&self) -> f64 {
        self.cursor / f64::from(self.audio.sample_rate.max(1))
    }
}

/// The sounds playing now, mixed at `rate` frames per second.
#[derive(Debug)]
pub struct Mixer<K> {
    rate: u32,
    voices: HashMap<K, Voice>,
}

impl<K: Hash + Eq + Copy> Mixer<K> {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate.max(1),
            voices: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.voices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.voices.is_empty()
    }

    /// Make the mixer play exactly `wanted`: new sounds start at their
    /// position, ones no longer wanted stop, and ones still playing keep
    /// their own position unless it drifted too far from the engine's.
    pub fn sync(&mut self, wanted: &[Play<K>]) {
        self.voices
            .retain(|k, _| wanted.iter().any(|p| p.key == *k));
        for p in wanted {
            let rate = f64::from(p.audio.sample_rate.max(1));
            let voice = self.voices.entry(p.key).or_insert_with(|| Voice {
                audio: p.audio.clone(),
                cursor: p.position * rate,
                gains: p.gains,
                target: p.gains,
                speed: p.speed,
                looped: p.looped,
                generation: p.generation,
            });
            if !Arc::ptr_eq(&voice.audio, &p.audio)
                || voice.generation != p.generation
                || (voice.seconds() - p.position).abs() > DRIFT_SECONDS
            {
                voice.audio = p.audio.clone();
                voice.cursor = p.position * rate;
                voice.generation = p.generation;
            }
            voice.target = p.gains;
            voice.speed = p.speed;
            voice.looped = p.looped;
        }
    }

    /// Mix into `out`, interleaved with `channels` channels, replacing what
    /// it held. Mono sounds go to every channel; stereo ones to the first
    /// two (and the left one to any more). A sound that ends stops unless
    /// it loops.
    pub fn mix(&mut self, out: &mut [f32], channels: usize) {
        out.fill(0.0);
        if channels == 0 {
            return;
        }
        let rate = f64::from(self.rate);
        self.voices.retain(|_, v| mix_voice(v, out, channels, rate));
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }
}

/// Add one voice to `out`; false once it has ended.
fn mix_voice(v: &mut Voice, out: &mut [f32], channels: usize, rate: f64) -> bool {
    let audio = &*v.audio;
    let frames = audio.frames();
    let src_channels = usize::from(audio.channels.max(1));
    if frames == 0 {
        return false;
    }
    // Linear interpolation between neighbouring source frames: cheap, and
    // fine for the archive's mostly 8-48 kHz effects (Approximation).
    let step = f64::from(audio.sample_rate) * f64::from(v.speed.max(0.0)) / rate;
    let sample = |frame: usize, ch: usize| f32::from(audio.samples[frame * src_channels + ch]);
    let count = (out.len() / channels).max(1) as f32;
    let (from, to) = (v.gains, v.target);
    v.gains = to;
    for (k, frame) in out.chunks_exact_mut(channels).enumerate() {
        let f = k as f32 / count;
        let gain = [0, 1].map(|c| (from[c] + (to[c] - from[c]) * f) / 32768.0);
        if v.cursor >= frames as f64 {
            if v.looped && step > 0.0 {
                v.cursor %= frames as f64;
            } else {
                return false;
            }
        }
        let i = v.cursor as usize;
        let next = if i + 1 < frames {
            i + 1
        } else if v.looped {
            0
        } else {
            i
        };
        let t = (v.cursor - i as f64) as f32;
        for (c, s) in frame.iter_mut().enumerate() {
            let ch = if c < src_channels { c } else { 0 };
            let a = sample(i, ch);
            let b = sample(next, ch);
            // A mono output takes both sides' average; channels past the
            // second take the left (Approximation).
            let g = match (channels, c) {
                (1, _) => (gain[0] + gain[1]) / 2.0,
                (_, 1) => gain[1],
                _ => gain[0],
            };
            *s += (a + (b - a) * t) * g;
        }
        v.cursor += step;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(rate: u32, samples: Vec<i16>) -> Arc<Audio> {
        Arc::new(Audio {
            format: None,
            sample_rate: rate,
            channels: 1,
            samples,
        })
    }

    fn play(audio: &Arc<Audio>, position: f64) -> Play<u32> {
        Play {
            key: 1,
            audio: audio.clone(),
            position,
            gains: [1.0, 1.0],
            speed: 1.0,
            looped: false,
            generation: 0,
        }
    }

    #[test]
    fn resamples_and_ends() {
        // 4 frames at 2 Hz, mixed at 4 Hz: every source frame twice, with
        // the half-way frames interpolated, then silence.
        let a = audio(2, vec![0, 16384, 0, -16384]);
        let mut m = Mixer::new(4);
        m.sync(&[play(&a, 0.0)]);
        let mut out = [9.0; 20];
        m.mix(&mut out, 2);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(
            left,
            [0.0, 0.25, 0.5, 0.25, 0.0, -0.25, -0.5, -0.5, 0.0, 0.0]
        );
        assert!(out.chunks(2).all(|f| f[0] == f[1]));
        assert!(m.is_empty());
    }

    #[test]
    fn follows_the_engine() {
        let a = audio(10, vec![1000; 100]);
        let mut m = Mixer::new(10);
        m.sync(&[play(&a, 0.0)]);
        let mut out = [0.0; 10];
        m.mix(&mut out, 1);
        // The engine says 1.1 s where the mixer is at 1.0: small drift
        // stays; a seek moves it; a sound no longer wanted stops.
        m.sync(&[play(&a, 1.1)]);
        assert!((m.voices[&1].seconds() - 1.0).abs() < 1e-9);
        m.sync(&[play(&a, 5.0)]);
        assert!((m.voices[&1].seconds() - 5.0).abs() < 1e-9);
        // Played again near where it was: starts over all the same.
        m.sync(&[Play {
            generation: 1,
            ..play(&a, 5.1)
        }]);
        assert!((m.voices[&1].seconds() - 5.1).abs() < 1e-9);
        m.sync(&[]);
        assert!(m.is_empty());
    }

    #[test]
    fn loops() {
        let a = audio(4, vec![8192, 0, 0, 0]);
        let mut m = Mixer::new(4);
        m.sync(&[Play {
            looped: true,
            ..play(&a, 0.0)
        }]);
        let mut out = [0.0; 9];
        m.mix(&mut out, 1);
        assert_eq!((out[0], out[4], out[8]), (0.25, 0.25, 0.25));
        assert_eq!(m.len(), 1);
    }

    /// Gains move to new values over one buffer, per side.
    #[test]
    fn ramps_and_pans() {
        let a = audio(4, vec![16384; 40]);
        let mut m = Mixer::new(4);
        m.sync(&[play(&a, 0.0)]);
        m.sync(&[Play {
            gains: [0.0, 1.0],
            ..play(&a, 0.0)
        }]);
        let mut out = [0.0; 8];
        m.mix(&mut out, 2);
        assert_eq!(out, [0.5, 0.5, 0.375, 0.5, 0.25, 0.5, 0.125, 0.5]);
        m.mix(&mut out, 2);
        assert_eq!(out, [0.0, 0.5, 0.0, 0.5, 0.0, 0.5, 0.0, 0.5]);
    }
}
