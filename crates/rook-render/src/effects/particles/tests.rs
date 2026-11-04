use std::sync::Arc;

use super::*;
use crate::scene::{Camera, Curve, Sequence};

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
        key: 1,
        position: Vec3::ZERO,
        rotation: Mat3::IDENTITY,
        enabled: true,
        time_scale: 1.0,
        emitted: 0,
        cleared: 0,
        look,
    }
}

/// `seconds` of frames at 60 a second, everything in view.
fn run(ps: &mut Particles, seconds: f32) {
    for _ in 0..(seconds * 60.0).round() as u32 {
        ps.advance(1.0 / 60.0);
        ps.frame(&Frustum::ALL, Vec3::ZERO);
    }
}

/// What a camera at the origin looking at `target` sees.
fn view(target: Vec3) -> Frustum {
    Frustum::of(Camera::look_at(Vec3::ZERO, target).view_projection(1.0))
}

/// A steady rate keeps rate × lifetime alive; disabling stops births
/// and the living ones die out; time scale 0 freezes them.
#[test]
fn rate_lifetime_and_enabled() {
    let mut e = emitter(look(20.0));
    let mut ps = Particles::default();
    ps.set(0, Some(&e));
    run(&mut ps, 3.0);
    let alive = ps.pools[0].particles.len();
    assert!((19..=21).contains(&alive), "{alive}");
    let top = ps.pools[0]
        .particles
        .iter()
        .map(|p| p.position.y)
        .fold(0.0, f32::max);
    assert!(top <= 1.0 && top > 0.9, "{top}");
    e.time_scale = 0.0;
    ps.set(0, Some(&e));
    run(&mut ps, 1.0);
    assert_eq!(ps.pools[0].particles.len(), alive);
    e.time_scale = 1.0;
    e.enabled = false;
    ps.set(0, Some(&e));
    run(&mut ps, 1.1);
    assert!(ps.pools[0].particles.is_empty());
}

/// `Emit` births at once, even disabled; `Clear` kills them all; a locked
/// emitter keeps its particles along its axes, so they go where it goes.
#[test]
fn emit_clear_and_locked() {
    let mut e = emitter(Arc::new(EmitterLook {
        locked: true,
        ..Default::default()
    }));
    e.enabled = false;
    let mut ps = Particles::default();
    e.emitted = 5;
    ps.set(0, Some(&e));
    assert_eq!(ps.pools[0].particles.len(), 5);
    // The same count again births nothing more.
    ps.set(0, Some(&e));
    assert_eq!(ps.pools[0].particles.len(), 5);
    e.position = Vec3::new(10.0, 0.0, 0.0);
    ps.set(0, Some(&e));
    let (at, particles) = ps.emitter(0).unwrap();
    assert!(
        particles
            .iter()
            .all(|p| (at.position + p.position).x == 10.0)
    );
    // Unlocked, they stay in the world where they were.
    let mut free = e.clone();
    free.look = Arc::new(EmitterLook::default());
    ps.set(0, Some(&free));
    assert!(ps.pools[0].particles.iter().all(|p| p.position.x == 10.0));
    e.cleared = 1;
    ps.set(0, Some(&e));
    assert!(ps.pools[0].particles.is_empty());
}

/// A scene built again keeps particles with their emitter's key, wherever
/// its slot is now; another emitter taking a slot starts afresh, and a
/// slot freed drops its particles.
#[test]
fn particles_follow_their_key() {
    let mut a = emitter(look(0.0));
    a.emitted = 3;
    let mut b = emitter(look(0.0));
    b.key = 2;
    b.emitted = 7;
    let mut ps = Particles::default();
    ps.replace(&[Some(a.clone()), Some(b.clone())]);
    ps.replace(&[Some(b.clone()), None]);
    assert_eq!(ps.pools[0].particles.len(), 7);
    assert!(ps.pools[1].particles.is_empty());
    ps.set(0, Some(&a));
    assert_eq!(
        ps.pools[0].particles.len(),
        3,
        "a new emitter bursts afresh"
    );
    ps.set(1, Some(&b));
    assert_eq!(ps.pools[1].particles.len(), 7);
    ps.set(0, None);
    assert_eq!(ps.pools[0].particles.len(), 0);
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

/// A busy emitter: births, acceleration, drag and spin all in play.
fn busy() -> Emitter {
    let mut e = emitter(Arc::new(EmitterLook {
        rate: 40.0,
        lifetime: [1.0, 3.0],
        speed: [2.0, 6.0],
        spread_angle: glam::Vec2::splat(30.0),
        world_acceleration: Vec3::new(0.0, -5.0, 1.0),
        drag: 0.5,
        spin: [10.0, 90.0],
        ..Default::default()
    }));
    e.position = Vec3::new(0.0, 0.0, -50.0);
    e
}

/// An emitter out of view does no work at all: nothing steps, nothing is
/// born or dies, and its particles stay exactly as they were.
#[test]
fn an_emitter_out_of_view_costs_nothing() {
    let e = busy();
    let mut ps = Particles::default();
    ps.set(0, Some(&e));
    let ahead = view(Vec3::NEG_Z);
    let behind = view(Vec3::Z);
    for _ in 0..60 {
        ps.advance(1.0 / 60.0);
        assert_eq!(ps.frame(&ahead, Vec3::ZERO), [0]);
    }
    let before = ps.pools[0].particles.clone();
    assert!(!before.is_empty());
    rook_types::counters::take_local();
    for _ in 0..120 {
        ps.advance(1.0 / 60.0);
        assert!(ps.frame(&behind, Vec3::ZERO).is_empty());
    }
    if rook_types::counters::ENABLED {
        let counts = rook_types::counters::take_local();
        assert_eq!(counts[rook_types::counters::Counter::ParticlesStepped], 0);
    }
    assert_eq!(ps.pools[0].particles, before);
}

/// Turning back shows the particles as a reference stepped only on the
/// frames they were in view shows them (Studio: their time stops while
/// out of view).
#[test]
fn turning_back_matches_a_stepped_reference() {
    let e = busy();
    let (ahead, behind) = (view(Vec3::NEG_Z), view(Vec3::Z));
    let mut seen = Particles::default();
    let mut reference = Particles::default();
    seen.set(0, Some(&e));
    reference.set(0, Some(&e));
    let frame = |ps: &mut Particles, view: &Frustum| {
        ps.advance(1.0 / 60.0);
        ps.frame(view, Vec3::ZERO);
    };
    for _ in 0..40 {
        frame(&mut seen, &ahead);
    }
    for _ in 0..90 {
        frame(&mut seen, &behind);
    }
    for _ in 0..40 {
        frame(&mut seen, &ahead);
    }
    for _ in 0..80 {
        frame(&mut reference, &Frustum::ALL);
    }
    assert!(!seen.pools[0].particles.is_empty());
    assert_eq!(seen.pools[0].particles, reference.pools[0].particles);
}

/// The step matches Studio's: 20 frames of 1/15 s (Studio's longest),
/// Speed 10 or Acceleration 10 along X, Drag 1. Fitted to Studio's
/// particles at 15 frames a second (docs/behavior/effects/particle-emitters.md).
#[test]
fn the_step_matches_studios() {
    let one = |speed: f32, acceleration: f32, drag: f32| {
        let mut e = emitter(Arc::new(EmitterLook {
            rate: 0.0,
            lifetime: [100.0, 100.0],
            speed: [speed, speed],
            direction: Vec3::X,
            world_acceleration: Vec3::X * acceleration,
            drag: drag * std::f32::consts::LN_2,
            ..Default::default()
        }));
        e.emitted = 1;
        let mut ps = Particles::default();
        ps.set(0, Some(&e));
        for _ in 0..20 {
            ps.advance(1.0);
            ps.frame(&Frustum::ALL, Vec3::ZERO);
        }
        ps.pools[0].particles[0].position.x
    };
    let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
    assert!(near(one(10.0, 0.0, 0.0), 13.333), "{}", one(10.0, 0.0, 0.0));
    assert!(near(one(0.0, 10.0, 0.0), 8.889), "{}", one(0.0, 10.0, 0.0));
    assert!(near(one(10.0, 0.0, 1.0), 8.904), "{}", one(10.0, 0.0, 1.0));
    assert!(near(one(0.0, 10.0, 1.0), 6.688), "{}", one(0.0, 10.0, 1.0));
}

/// Studio's draw budget, in its own cases: a 10 × 10 grid of emitters seen
/// from above its middle.
#[test]
fn the_draw_budget_takes_whole_emitters_nearest_first() {
    let grid = |each: &dyn Fn(usize) -> u64| {
        let mut ps = Particles::default();
        for i in 0..100 {
            let mut e = emitter(Arc::new(EmitterLook::default()));
            e.key = i as u64 + 1;
            e.enabled = false;
            e.position = Vec3::new((i % 10) as f32 * 10.0, 50.0, (i / 10) as f32 * 10.0);
            e.emitted = each(i);
            ps.set(i, Some(&e));
        }
        let mut slots: Vec<usize> = (0..100).collect();
        ps.within_budget(&mut slots, Vec3::new(45.0, 160.0, 45.0));
        slots
    };
    assert_eq!(grid(&|_| 161).len(), 99);
    assert_eq!(grid(&|_| 300).len(), 53);
    assert_eq!(grid(&|_| 1000).len(), 16);
    let middle = |i: usize| (i % 10 == 4 || i % 10 == 5) && (i / 10 == 4 || i / 10 == 5);
    let mixed = grid(&|i| if middle(i) { 5000 } else { 100 });
    assert_eq!(mixed.iter().filter(|&&i| middle(i)).count(), 3);
    assert_eq!(mixed.len(), 13);
}

/// `Rate` thins with distance as Studio's did.
#[test]
fn rate_thins_with_distance() {
    assert_eq!(rate_share(100.0), 1.0);
    assert!((rate_share(300.0) - 0.82).abs() < 0.02);
    assert!((rate_share(500.0) - 0.59).abs() < 0.02);
    assert!((rate_share(900.0) - 0.12).abs() < 0.02);
    assert_eq!(rate_share(1200.0), 0.0);
}
