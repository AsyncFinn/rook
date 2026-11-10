//! Fire, Smoke and Sparkles as the renderer's emitters, each fitted to
//! Studio 0.742 (docs/behavior/effects/fire-smoke-and-sparkles.md).

use rook_render::{Curve, EmitterLook, Sequence, Spawn};

use super::*;

/// Keypoints from a table of values at even steps over a particle's life.
fn even(values: &[f32]) -> Curve {
    let last = (values.len() - 1).max(1) as f32;
    Curve(
        values
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as f32 / last, v))
            .collect(),
    )
}

/// Fire: flames in `Color` that rise faster the higher `Heat` (its
/// acceleration in studs/s², halved), as wide as about half its `Size`,
/// covering what's behind and glowing; and a smaller, longer-lived core
/// and trail in `SecondaryColor` added on top. Neither is lit.
pub(super) fn fire(dom: &Dom, id: InstanceId, textures: &EffectTextures) -> Vec<EmitterLook> {
    let size = dom
        .float_property(id, "Size")
        .unwrap_or(5.0)
        .clamp(2.0, 30.0);
    let heat = dom
        .float_property(id, "Heat")
        .unwrap_or(9.0)
        .clamp(-25.0, 25.0);
    let primary = dom
        .color3_property(id, "Color")
        .map(Color3::to_array)
        .unwrap_or([0.925, 0.545, 0.275]);
    let secondary = dom
        .color3_property(id, "SecondaryColor")
        .map(Color3::to_array)
        .unwrap_or([0.545, 0.314, 0.216]);
    let rise = Vec3::new(0.0, heat * 0.5, 0.0);
    let flames = EmitterLook {
        texture: Some(textures.fire),
        rate: 40.0,
        lifetime: [1.0, 1.4],
        spawn: Spawn::Ball(0.12 * size),
        spread: 0.05 * size,
        acceleration: rise,
        size: Sequence::from(Curve(vec![(0.0, 0.4 * size), (1.0, 0.15 * size)])),
        size_spread: [0.8, 1.2],
        spin: [0.0, 60.0],
        spin_either_way: true,
        color: Curve::constant(primary),
        brightness: Curve(vec![(0.0, 20.0), (0.5, 14.0), (0.75, 2.0), (1.0, 0.0)]),
        alpha: Sequence::from(Curve(vec![(0.0, 0.0), (0.1, 1.0), (0.7, 0.8), (1.0, 0.0)])),
        ..Default::default()
    };
    let core = EmitterLook {
        texture: Some(textures.fire),
        rate: 40.0,
        lifetime: [1.3, 1.9],
        spawn: Spawn::Ball(0.05 * size),
        spread: 0.05 * size,
        acceleration: rise * 1.1,
        size: Sequence::from(Curve(vec![(0.0, 0.3 * size), (1.0, 0.1 * size)])),
        size_spread: [0.8, 1.2],
        spin: [0.0, 60.0],
        spin_either_way: true,
        color: Curve::constant(secondary),
        brightness: Curve::constant(4.0),
        alpha: Sequence::from(Curve(vec![(0.0, 0.0), (0.15, 0.8), (0.6, 0.5), (1.0, 0.0)])),
        light_emission: 1.0,
        ..Default::default()
    };
    vec![flames, core]
}

/// Smoke: big, lit puffs in `Color` rising at `RiseVelocity` and growing
/// from `Size` by about 4 studs a second, as opaque as `Opacity` at their
/// thickest, a third of the way through their 5 seconds.
pub(super) fn smoke(dom: &Dom, id: InstanceId, textures: &EffectTextures) -> EmitterLook {
    let size = dom
        .float_property(id, "Size")
        .unwrap_or(1.0)
        .clamp(0.1, 100.0);
    let opacity = dom
        .float_property(id, "Opacity")
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    let rise = dom
        .float_property(id, "RiseVelocity")
        .unwrap_or(1.0)
        .clamp(-25.0, 25.0);
    // The install's smoke ramp (`textures/particles/smoke_color.dds`
    // alpha), in sixteenths of the life.
    let ramp = [
        0.02, 0.07, 0.22, 0.42, 0.58, 0.63, 0.57, 0.45, 0.32, 0.22, 0.15, 0.11, 0.07, 0.05, 0.02,
        0.0,
    ];
    let alpha: Vec<f32> = ramp.iter().map(|a| (a * opacity * 3.6).min(1.0)).collect();
    EmitterLook {
        texture: Some(textures.smoke),
        rate: 10.0,
        lifetime: [4.5, 5.5],
        spawn: Spawn::Ball(0.1 * size),
        velocity: Vec3::new(0.0, rise, 0.0),
        spread: 3.0,
        drag: 0.4,
        size: Sequence::from(Curve(vec![(0.0, size + 1.0), (1.0, size + 14.0)])),
        size_spread: [0.8, 1.2],
        spin: [0.0, 20.0],
        spin_either_way: true,
        color: Curve::constant(
            dom.color3_property(id, "Color")
                .map(Color3::to_array)
                .unwrap_or([1.0; 3]),
        ),
        brightness: Curve::constant(1.0),
        alpha: Sequence::from(even(&alpha)),
        light_influence: 1.0,
        ..Default::default()
    }
}

/// Sparkles: stars in `SparkleColor` thrown out a few studs every way
/// (more upward), lighting what's behind them.
pub(super) fn sparkles(dom: &Dom, id: InstanceId, textures: &EffectTextures) -> EmitterLook {
    EmitterLook {
        texture: Some(textures.sparkles),
        rate: 30.0,
        lifetime: [1.0, 1.6],
        spawn: Spawn::Ball(0.5),
        velocity: Vec3::new(0.0, 2.0, 0.0),
        spread: 8.0,
        drag: 1.0,
        size: Sequence::from(Curve::constant(1.6)),
        size_spread: [0.4, 1.4],
        spin: [0.0, 90.0],
        spin_either_way: true,
        color: Curve::constant(
            dom.color3_property(id, "SparkleColor")
                .map(Color3::to_array)
                .unwrap_or([0.565, 0.098, 1.0]),
        ),
        brightness: Curve::constant(1.8),
        alpha: Sequence::from(Curve(vec![
            (0.0, 0.0),
            (0.2, 0.1),
            (0.45, 0.85),
            (0.7, 0.55),
            (1.0, 0.0),
        ])),
        light_emission: 1.0,
        ..Default::default()
    }
}
