//! Where a 3D sound is heard from: how loud by distance (`RollOffMode`)
//! and how it splits between left and right, as measured from Studio's own
//! output (`docs/behavior/audio.md`, "3D sound").
//!
//! Directions are in the listener's space, Roblox's camera axes: -Z ahead,
//! +X right, +Y up.

use glam::Vec3;

/// `Enum.RollOffMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollOff {
    Inverse,
    Linear,
    LinearSquare,
    InverseTapered,
}

impl RollOff {
    /// By `Enum.RollOffMode` value; unknown values are Inverse, the
    /// default.
    pub fn from_value(v: u32) -> Self {
        match v {
            1 => Self::Linear,
            2 => Self::LinearSquare,
            3 => Self::InverseTapered,
            _ => Self::Inverse,
        }
    }
}

/// How loud a sound `distance` studs away is, 0 to 1 (Confirmed against
/// Studio's output for every mode, and creator-docs `RollOffMode`):
///
/// - nothing past `max`, whatever `min` is, and full volume within `min`;
/// - Inverse: `min / d`; Linear: `(max - d) / (max - min)`; LinearSquare:
///   Linear squared; InverseTapered: the lesser of Inverse and
///   LinearSquare;
/// - `SoundService.RolloffScale` stretches the distance past `min`:
///   `d = min + scale * (distance - min)`.
pub fn attenuation(mode: RollOff, distance: f32, min: f32, max: f32, scale: f32) -> f32 {
    if distance > max {
        return 0.0;
    }
    if distance <= min {
        return 1.0;
    }
    let d = min + scale.max(0.0) * (distance - min);
    let inverse = if d > 0.0 { (min / d).min(1.0) } else { 1.0 };
    let linear = if max > min {
        ((max - d) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    match mode {
        RollOff::Inverse => inverse,
        RollOff::Linear => linear,
        RollOff::LinearSquare => linear * linear,
        RollOff::InverseTapered => inverse.min(linear * linear),
    }
}

/// The 7.1 speakers Studio pans between, by angle (degrees, clockwise from
/// ahead), with how much of each reaches the left and right of a stereo
/// output. Fitted to Studio's stereo output for a source at every 30
/// degrees: a source at 30 degrees came out at 1.41 on the right, at 60
/// degrees 1.71, at 90 degrees 1.0, ahead 1.0 on both and behind 0.71 on
/// both, which is constant-power panning between these speakers folded
/// down with these weights (Confirmed for a stereo device; other layouts
/// are unknown).
const SPEAKERS: [(f32, [f32; 2]); 7] = [
    (-150.0, [1.0, 0.0]),
    (-90.0, [1.0, 0.0]),
    (-30.0, [std::f32::consts::SQRT_2, 0.0]),
    (0.0, [1.0, 1.0]),
    (30.0, [0.0, std::f32::consts::SQRT_2]),
    (90.0, [0.0, 1.0]),
    (150.0, [0.0, 1.0]),
];

/// Left and right gains for a sound in direction `local` from the
/// listener. A sound straight above, below or at the listener came out at
/// 0.89 on both sides; one partly above leans towards that (Inference).
pub fn pan(local: Vec3) -> [f32; 2] {
    const OVERHEAD: f32 = 0.89;
    let length = local.length();
    if length < 1e-4 {
        return [OVERHEAD; 2];
    }
    let flat = Vec3::new(local.x, 0.0, local.z).length();
    let up = local.y.abs() / length;
    if flat < 1e-4 {
        return [OVERHEAD; 2];
    }
    let angle = local.x.atan2(-local.z).to_degrees();
    let around = ring(angle);
    [0, 1].map(|c| around[c] * (1.0 - up) + OVERHEAD * up)
}

/// Constant-power panning between the two speakers either side of `angle`.
fn ring(angle: f32) -> [f32; 2] {
    let n = SPEAKERS.len();
    for i in 0..n {
        let (a, ga) = SPEAKERS[i];
        let (mut b, gb) = SPEAKERS[(i + 1) % n];
        if b < a {
            b += 360.0;
        }
        let mut x = angle;
        if x < a {
            x += 360.0;
        }
        if x >= a && x <= b {
            let t = (x - a) / (b - a) * std::f32::consts::FRAC_PI_2;
            let (wa, wb) = (t.cos(), t.sin());
            return [0, 1].map(|c| ga[c] * wa + gb[c] * wb);
        }
    }
    [1.0, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.02
    }

    /// The levels Studio's output had (min 10, max 100, and the default
    /// max 10000).
    #[test]
    fn roll_off_matches_studio() {
        use RollOff::*;
        for (mode, d, want) in [
            (Inverse, 5.0, 1.0),
            (Inverse, 20.0, 0.497),
            (Inverse, 60.0, 0.166),
            (Inverse, 120.0, 0.0),
            (Linear, 40.0, 0.667),
            (Linear, 95.0, 0.055),
            (LinearSquare, 40.0, 0.444),
            (LinearSquare, 60.0, 0.197),
            (InverseTapered, 20.0, 0.496),
            (InverseTapered, 80.0, 0.049),
        ] {
            let got = attenuation(mode, d, 10.0, 100.0, 1.0);
            assert!(near(got, want), "{mode:?} at {d}: {got}");
        }
        assert!(near(attenuation(Inverse, 40.0, 10.0, 100.0, 2.0), 0.143));
        assert!(near(attenuation(Inverse, 40.0, 10.0, 100.0, 0.5), 0.4));
        assert!(near(attenuation(Linear, 40.0, 10.0, 100.0, 2.0), 0.333));
        assert_eq!(attenuation(Inverse, 40.0, 0.0, 100.0, 1.0), 0.0);
        // Past max is silent even within min.
        assert_eq!(attenuation(Inverse, 10.0, 20.0, 3.0, 1.0), 0.0);
    }

    #[test]
    fn pan_matches_studio() {
        let at = |deg: f32| {
            let r = deg.to_radians();
            pan(Vec3::new(r.sin(), 0.0, -r.cos()))
        };
        for (deg, l, r) in [
            (0.0, 1.0, 1.0),
            (30.0, 0.0, 1.414),
            (45.0, 0.0, 1.672),
            (60.0, 0.0, 1.707),
            (90.0, 0.0, 1.0),
            (120.0, 0.0, 1.414),
            (180.0, 0.707, 0.707),
            (240.0, 1.414, 0.0),
            (300.0, 1.707, 0.0),
        ] {
            let [gl, gr] = at(deg);
            assert!(near(gl, l) && near(gr, r), "{deg}: {gl} {gr}");
        }
        assert_eq!(pan(Vec3::new(0.0, -10.0, 0.0)), [0.89, 0.89]);
        assert_eq!(pan(Vec3::ZERO), [0.89, 0.89]);
    }
}
