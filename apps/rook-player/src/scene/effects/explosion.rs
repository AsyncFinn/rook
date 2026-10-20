//! Explosion's look: one burst of particles at its `Position`, the same
//! size whatever its `BlastRadius`, fitted to Studio 0.742
//! (docs/behavior/effects/explosions.md).

use rook_render::{Curve, EmitterLook, EmitterShape, InOut, Sequence, Spawn};

use super::*;

/// The emitters Explosion `id` draws with: always as many, drawing
/// nothing when it isn't `Visible`.
pub(super) fn emitters(dom: &Dom, id: InstanceId, textures: &EffectTextures) -> Vec<Emitter> {
    let position = vector3(dom, id, "Position").unwrap_or(Vec3::ZERO);
    let visible = boolean(dom, id, "Visible") != Some(false);
    let time_scale = number(dom, id, "TimeScale").unwrap_or(1.0).clamp(0.0, 1.0);
    looks(textures)
        .into_iter()
        .enumerate()
        .map(|(layer, (burst, mut look))| {
            if !visible {
                look.texture = None;
            }
            Emitter {
                key: key(id, layer),
                position,
                rotation: Mat3::IDENTITY,
                enabled: false,
                time_scale,
                emitted: burst,
                cleared: 0,
                look: Arc::new(look),
            }
        })
        .collect()
}

/// Each layer's burst and look. Studio's, by stage: a white flash about
/// 30 studs across for a third of a second; a yellow-white fireball 8
/// studs across turning orange, dark red and brown as it swells to 13 by
/// 0.3 s; dark smoke from it 18 studs across by 0.6 s, black by 1 s,
/// rising about 4 studs a second and gone by 2.7 s; a hundred pale specks
/// thrown 20 studs by 0.3 s and 30 by 1 s; and a faint ring sweeping out
/// past 40 studs.
fn looks(textures: &EffectTextures) -> [(u64, EmitterLook); 5] {
    let flash = EmitterLook {
        texture: Some(textures.explosion_flash),
        lifetime: [0.6, 0.6],
        size: Sequence::from(Curve(vec![(0.0, 36.0), (1.0, 44.0)])),
        color: Curve::constant([1.0; 3]),
        brightness: Curve::constant(3.0),
        alpha: Sequence::from(Curve(vec![(0.0, 1.0), (0.5, 0.5), (1.0, 0.0)])),
        light_emission: 1.0,
        ..Default::default()
    };
    let fireball = EmitterLook {
        texture: Some(textures.explosion_core),
        lifetime: [0.6, 0.8],
        spawn: Spawn::Ball(1.5),
        spread: 4.0,
        size: Sequence::from(Curve(vec![(0.0, 7.0), (1.0, 16.0)])),
        size_spread: [0.85, 1.15],
        spin: [0.0, 40.0],
        spin_either_way: true,
        color: Curve(vec![
            (0.0, [1.0, 0.95, 0.8]),
            (0.15, [1.0, 0.7, 0.4]),
            (0.4, [0.7, 0.38, 0.2]),
            (1.0, [0.35, 0.15, 0.07]),
        ]),
        brightness: Curve(vec![(0.0, 6.0), (0.15, 3.0), (0.4, 1.0), (1.0, 0.7)]),
        alpha: Sequence::from(Curve(vec![(0.0, 1.0), (0.7, 1.0), (1.0, 0.0)])),
        ..Default::default()
    };
    let smoke = EmitterLook {
        texture: Some(textures.explosion_smoke),
        lifetime: [2.0, 2.6],
        spawn: Spawn::Ball(2.0),
        velocity: Vec3::new(0.0, 3.0, 0.0),
        spread: 7.0,
        drag: 1.2,
        world_acceleration: Vec3::new(0.0, 2.0, 0.0),
        size: Sequence::from(Curve(vec![(0.0, 6.0), (0.3, 14.0), (1.0, 20.0)])),
        size_spread: [0.85, 1.15],
        spin: [0.0, 20.0],
        spin_either_way: true,
        color: Curve(vec![
            (0.0, [0.45, 0.15, 0.05]),
            (0.25, [0.3, 0.1, 0.03]),
            (0.5, [0.06, 0.03, 0.02]),
            (1.0, [0.05, 0.05, 0.05]),
        ]),
        alpha: Sequence::from(Curve(vec![(0.0, 0.0), (0.12, 1.0), (0.6, 0.9), (1.0, 0.0)])),
        ..Default::default()
    };
    let specks = EmitterLook {
        texture: Some(textures.explosion_smoke),
        lifetime: [1.0, 1.8],
        spawn: Spawn::Shape {
            shape: EmitterShape::Sphere,
            half: Vec3::splat(0.5),
            surface: false,
            in_out: InOut::Outward,
            partial: 1.0,
        },
        speed: [40.0, 90.0],
        drag: 2.5,
        world_acceleration: Vec3::new(0.0, -10.0, 0.0),
        size: Sequence::constant(0.8),
        size_spread: [0.6, 1.4],
        color: Curve::constant([1.0, 0.82, 0.72]),
        brightness: Curve::constant(1.5),
        alpha: Sequence::from(Curve(vec![(0.0, 0.8), (0.7, 0.6), (1.0, 0.0)])),
        light_emission: 0.3,
        ..Default::default()
    };
    let shockwave = EmitterLook {
        texture: Some(textures.explosion_ring),
        lifetime: [0.9, 0.9],
        size: Sequence::from(Curve(vec![(0.0, 10.0), (1.0, 150.0)])),
        rotation: [0.0, 0.0],
        color: Curve::constant([0.85, 0.92, 1.0]),
        alpha: Sequence::from(Curve(vec![(0.0, 0.5), (1.0, 0.0)])),
        light_emission: 1.0,
        ..Default::default()
    };
    [
        (1, flash),
        (10, fireball),
        (14, smoke),
        (160, specks),
        (1, shockwave),
    ]
}
