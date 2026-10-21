//! Where a 3D sound is heard from: how loud by distance (`RollOffMode`)
//! and how it splits between left and right, as measured from Studio's own
//! output (`docs/behavior/audio/3d-sound.md`).
//!
//! Directions are in the listener's space, Roblox's camera axes: -Z ahead,
//! +X right, +Y up.

use glam::Vec3;
use rook_types::RollOffMode;

/// How loud a sound `distance` studs away is, 0 to 1 (Confirmed against
/// Studio's output for every mode, and creator-docs `RollOffMode`):
///
/// - nothing past `max`, whatever `min` is, and full volume within `min`;
/// - Inverse: `min / d`; Linear: `(max - d) / (max - min)`; LinearSquare:
///   Linear squared; InverseTapered: the lesser of Inverse and
///   LinearSquare;
/// - `SoundService.RolloffScale` stretches the distance past `min`:
///   `d = min + scale * (distance - min)`.
pub fn attenuation(mode: RollOffMode, distance: f32, min: f32, max: f32, scale: f32) -> f32 {
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
        RollOffMode::Inverse => inverse,
        RollOffMode::Linear => linear,
        RollOffMode::LinearSquare => linear * linear,
        RollOffMode::InverseTapered => inverse.min(linear * linear),
    }
}

/// How loud an AudioEmitter is `distance` studs from a listener, 0 to 1,
/// by its `DistanceAttenuationMode` (`None` for Custom, otherwise the
/// RollOffMode formula it names), its `DistanceAttenuationBounds` and the
/// curve `SetDistanceAttenuation` gave it (Confirmed against Studio's
/// `GetAudibilityFor`):
///
/// - Custom with a curve: the curve ([`curve`]);
/// - Custom with no curve: InverseTapered between 4 and 10000 studs,
///   whatever the bounds are (`min / d` up to about 9,500 studs, then
///   falling to 0 at 10,000);
/// - the others: the RollOffMode formulas between the bounds, but silent
///   from the maximum on (a Sound is silent only past it).
pub fn emitter_attenuation(
    mode: Option<RollOffMode>,
    bounds: [f32; 2],
    points: &[(f32, f32)],
    distance: f32,
) -> f32 {
    let preset = |roll_off, [min, max]: [f32; 2]| {
        if distance >= max {
            0.0
        } else {
            attenuation(roll_off, distance, min, max, 1.0)
        }
    };
    match mode {
        None if points.is_empty() => preset(RollOffMode::InverseTapered, [4.0, 10_000.0]),
        None => curve(points, distance),
        Some(roll_off) => preset(roll_off, bounds),
    }
}

/// A volume-over-distance curve's level at `distance`: straight lines
/// between its points (distance, volume), flat past either end; 1 with no
/// points (creator-docs `SetDistanceAttenuation`, Confirmed by
/// `GetAudibilityFor`). `points` are sorted by distance.
pub fn curve(points: &[(f32, f32)], distance: f32) -> f32 {
    let Some(&(first_d, first_v)) = points.first() else {
        return 1.0;
    };
    if distance <= first_d {
        return first_v;
    }
    for pair in points.windows(2) {
        let [(d0, v0), (d1, v1)] = [pair[0], pair[1]];
        if distance <= d1 {
            let t = if d1 > d0 {
                (distance - d0) / (d1 - d0)
            } else {
                1.0
            };
            return v0 + (v1 - v0) * t;
        }
    }
    points.last().map_or(1.0, |p| p.1)
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
        use RollOffMode::*;
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

    /// `GetAudibilityFor` in Studio 0.742, for an emitter with no curve,
    /// with the presets between 10 and 100, and with a curve.
    #[test]
    fn emitter_attenuation_matches_studio() {
        let none: &[(f32, f32)] = &[];
        let b = [10.0, 100.0];
        use RollOffMode::*;
        for (mode, d, want) in [
            (None, 4.0, 1.0),
            (None, 10.0, 0.4),
            (None, 1000.0, 0.004),
            (None, 9900.0, 0.0001),
            (Some(Inverse), 95.0, 0.1053),
            (Some(Inverse), 100.0, 0.0),
            (Some(Linear), 15.0, 0.9444),
            (Some(LinearSquare), 80.0, 0.0494),
            (Some(InverseTapered), 60.0, 0.1667),
            (Some(InverseTapered), 95.0, 0.0031),
        ] {
            let got = emitter_attenuation(mode, b, none, d);
            assert!((got - want).abs() < 2e-4, "mode {mode:?} at {d}: {got}");
        }
        let c = [(10.0, 1.0), (100.0, 0.4), (300.0, 0.0)];
        for (d, want) in [(0.0, 1.0), (55.0, 0.7), (200.0, 0.2), (500.0, 0.0)] {
            assert!((emitter_attenuation(None, b, &c, d) - want).abs() < 1e-5);
        }
        assert_eq!(curve(&[(50.0, 0.5)], 0.0), 0.5);
        assert_eq!(curve(&[], 7.0), 1.0);
    }
}
