//! DistortionSoundEffect: soft clipping whose strength is `Level`, 0 (none)
//! to 1 (a square wave).
//!
//! The curve is `(1 + k) x / (1 + k |x|)` with `k = 2 level / (1 - level)`
//! (Inference: the waveshaper FMOD's distortion is known to use; Studio's
//! levels for a recording at every quarter of `Level` match it, see
//! `docs/behavior/audio/soundgroups-and-effects.md`).

use super::Frame;

pub(super) fn process(level: f32, frames: &mut [Frame]) {
    let level = level.clamp(0.0, 1.0);
    if level == 0.0 {
        return;
    }
    let level = level.min(0.9999);
    let k = 2.0 * level / (1.0 - level);
    for s in frames.iter_mut().flatten() {
        *s = (1.0 + k) * *s / (1.0 + k * s.abs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_softly() {
        let mut f = [[0.25, -0.25], [1.0, 0.0]];
        process(0.0, &mut f);
        assert_eq!(f, [[0.25, -0.25], [1.0, 0.0]]);
        process(0.5, &mut f);
        // k = 2: 0.25 → 0.75 / 1.5; full scale stays full scale.
        assert_eq!(f, [[0.5, -0.5], [1.0, 0.0]]);
        let mut g = [[0.01, -0.001]];
        process(1.0, &mut g);
        assert!(g[0][0] > 0.99 && g[0][1] < -0.9);
    }
}
