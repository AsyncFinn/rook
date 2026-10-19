use super::*;
use crate::scene::{Curve, Sequence};

fn look(rate: f32) -> Arc<EmitterLook> {
    Arc::new(EmitterLook {
        texture: None,
        rate,
        speed: [1.0, 1.0],
        ..Default::default()
    })
}

fn emitter(look: Arc<EmitterLook>) -> Emitter {
    Emitter {
        position: Vec3::ZERO,
        rotation: Mat3::IDENTITY,
        enabled: true,
        time_scale: 1.0,
        emitted: 0,
        cleared: 0,
        look,
    }
}

/// A steady rate keeps rate × lifetime alive; disabling stops births
/// and the living ones die out; time scale 0 freezes them.
#[test]
fn rate_lifetime_and_enabled() {
    let mut e = emitter(look(20.0));
    let mut ps = Particles::default();
    ps.set_emitters(std::slice::from_ref(&e));
    for _ in 0..3 {
        ps.step(1.0);
    }
    let alive = ps.pools[0].particles.len();
    assert!((19..=21).contains(&alive), "{alive}");
    let top = ps.pools[0]
        .particles
        .iter()
        .map(|p| p.position.y)
        .fold(0.0, f32::max);
    assert!(top <= 1.0 && top > 0.9, "{top}");
    e.time_scale = 0.0;
    ps.set_emitters(std::slice::from_ref(&e));
    ps.step(1.0);
    assert_eq!(ps.pools[0].particles.len(), alive);
    e.time_scale = 1.0;
    e.enabled = false;
    ps.set_emitters(std::slice::from_ref(&e));
    ps.step(1.0);
    assert!(!ps.any());
}

/// `Emit` births at once, even disabled; `Clear` kills them all; a locked
/// emitter carries its particles when it moves.
#[test]
fn emit_clear_and_locked() {
    let mut e = emitter(Arc::new(EmitterLook {
        locked: true,
        ..Default::default()
    }));
    e.enabled = false;
    let mut ps = Particles::default();
    e.emitted = 5;
    ps.set_emitters(std::slice::from_ref(&e));
    assert_eq!(ps.pools[0].particles.len(), 5);
    // The same count again births nothing more.
    ps.set_emitters(std::slice::from_ref(&e));
    assert_eq!(ps.pools[0].particles.len(), 5);
    e.position = Vec3::new(10.0, 0.0, 0.0);
    ps.set_emitters(std::slice::from_ref(&e));
    assert!(ps.pools[0].particles.iter().all(|p| p.position.x == 10.0));
    e.cleared = 1;
    ps.set_emitters(std::slice::from_ref(&e));
    assert!(!ps.any());
}

#[test]
fn curves_and_squash() {
    let c = Curve(vec![(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)]);
    assert_eq!(c.at(0.25), 0.5);
    assert_eq!(c.at(0.75), 0.5);
    assert_eq!(c.at(2.0), 0.0);
    let look = EmitterLook {
        size: Sequence::constant(4.0),
        squash: Curve::constant(1.0),
        ..Default::default()
    };
    let mut pool = Pool::default();
    pool.birth(&emitter(Arc::new(look.clone())), 0.0);
    assert_eq!(pool.particles[0].size(&look), [2.0, 8.0]);
}
