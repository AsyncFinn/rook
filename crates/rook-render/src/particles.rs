//! The particles of the scene's emitters, simulated on the CPU in world
//! space: each emitter's are born at its rate where it is, move by their
//! velocity, acceleration and drag, and die at the end of their lifetime
//! (docs/behavior/effects.md).

use std::sync::Arc;

use glam::Vec3;

use crate::batch::cull::Aabb;
use crate::scene::{Emitter, EmitterLook};

/// The longest step simulated at once; longer ones are split, up to
/// [`MAX_STEP`] in all.
const SUBSTEP: f32 = 1.0 / 30.0;
/// The most time one call simulates (a frame after a long stall).
const MAX_STEP: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Particle {
    pub(crate) position: Vec3,
    velocity: Vec3,
    age: f32,
    lifetime: f32,
    /// The random factor on its size.
    scale: f32,
    /// Degrees.
    rotation: f32,
    spin: f32,
}

impl Particle {
    /// Its share of its lifetime gone, 0 to 1.
    pub(crate) fn life(&self) -> f32 {
        (self.age / self.lifetime).clamp(0.0, 1.0)
    }
}

/// One emitter's living particles.
#[derive(Debug, Default)]
struct Pool {
    particles: Vec<Particle>,
    /// Particles owed but not yet born (a fraction of one).
    owed: f32,
    rng: Rng,
    /// A box around its particles, as of the last step.
    bounds: Option<Aabb>,
}

/// Every emitter's particles, by the emitter's index in the scene.
#[derive(Debug, Default)]
pub(crate) struct Particles {
    emitters: Vec<Emitter>,
    pools: Vec<Pool>,
}

impl Particles {
    /// Take the scene's emitters (by index: an emitter's particles stay
    /// with its index).
    pub(crate) fn set_emitters(&mut self, emitters: &[Emitter]) {
        self.emitters.clear();
        self.emitters.extend_from_slice(emitters);
        while self.pools.len() < emitters.len() {
            let seed = self.pools.len() as u64;
            self.pools.push(Pool {
                rng: Rng::new(seed),
                ..Default::default()
            });
        }
        // Emitters gone: their particles go with them.
        self.pools.truncate(emitters.len());
    }

    /// Move every particle on by `dt` seconds (each emitter's
    /// `time_scale` of it), birthing and killing as they go.
    pub(crate) fn step(&mut self, dt: f32) {
        profiling::scope!("Particles::step");
        let mut left = dt.clamp(0.0, MAX_STEP);
        while left > 0.0 {
            let h = left.min(SUBSTEP);
            left -= h;
            for (emitter, pool) in self.emitters.iter().zip(&mut self.pools) {
                pool.step(emitter, h * emitter.time_scale.max(0.0));
            }
        }
        for pool in &mut self.pools {
            pool.bounds = pool.bounds();
        }
    }

    /// Whether any particle is alive.
    pub(crate) fn any(&self) -> bool {
        self.pools.iter().any(|p| !p.particles.is_empty())
    }

    /// Each emitter's look, its particles and the box around them.
    pub(crate) fn drawn(&self) -> impl Iterator<Item = (&Arc<EmitterLook>, &[Particle], Aabb)> {
        self.emitters
            .iter()
            .zip(&self.pools)
            .filter_map(|(e, p)| Some((&e.look, p.particles.as_slice(), p.bounds?)))
    }
}

impl Pool {
    fn step(&mut self, emitter: &Emitter, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let look = &*emitter.look;
        let damping = (-look.drag * dt).exp();
        let acceleration = emitter.rotation * look.acceleration;
        for p in &mut self.particles {
            p.age += dt;
            p.velocity = (p.velocity + acceleration * dt) * damping;
            p.position += p.velocity * dt;
            p.rotation += p.spin * dt;
        }
        self.particles.retain(|p| p.age < p.lifetime);
        if !emitter.enabled || look.rate <= 0.0 {
            self.owed = 0.0;
            return;
        }
        self.owed += look.rate * dt;
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            // Born some way into the step, so a steady rate doesn't
            // come out in layers.
            let age = self.rng.next() * dt;
            let r = &mut self.rng;
            let mut p = Particle {
                position: emitter.position + r.in_ball() * look.spawn_radius,
                velocity: emitter.rotation * look.velocity + r.in_ball() * look.spread,
                age: 0.0,
                lifetime: r.between(look.lifetime),
                scale: r.between(look.size_spread),
                rotation: r.next() * 360.0,
                spin: r.between(look.spin) * if r.next() < 0.5 { -1.0 } else { 1.0 },
            };
            p.age = age;
            p.velocity += acceleration * age;
            p.position += p.velocity * age;
            self.particles.push(p);
        }
    }

    /// A box around its particles (their full size).
    fn bounds(&self) -> Option<Aabb> {
        let mut b = Aabb::EMPTY;
        for p in &self.particles {
            b = b.union(Aabb {
                min: p.position,
                max: p.position,
            });
        }
        (!b.is_empty()).then_some(b)
    }
}

impl Particle {
    /// Its width in studs now.
    pub(crate) fn size(&self, look: &EmitterLook) -> f32 {
        look.size.at(self.life()) * self.scale
    }

    pub(crate) fn rotation(&self) -> f32 {
        self.rotation
    }
}

/// A small, fast random number generator (SplitMix64), seeded per
/// emitter so that the same scene draws the same particles.
#[derive(Debug, Clone, Default)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x2545_F491_4F6C_DD1D)
    }

    /// A number in [0, 1).
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }

    fn between(&mut self, [a, b]: [f32; 2]) -> f32 {
        a + (b - a) * self.next()
    }

    /// A point in the unit ball, evenly.
    fn in_ball(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(self.next(), self.next(), self.next()) * 2.0 - Vec3::ONE;
            if v.length_squared() <= 1.0 {
                return v;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Curve;

    fn look(rate: f32) -> Arc<EmitterLook> {
        Arc::new(EmitterLook {
            texture: None,
            rate,
            lifetime: [1.0, 1.0],
            spawn_radius: 0.0,
            velocity: Vec3::Y,
            spread: 0.0,
            acceleration: Vec3::ZERO,
            drag: 0.0,
            size: Curve::constant(1.0),
            size_spread: [1.0, 1.0],
            spin: [0.0, 0.0],
            color: [1.0; 3],
            brightness: Curve::constant(1.0),
            alpha: Curve::constant(1.0),
            light_emission: 0.0,
            lit: false,
        })
    }

    /// A steady rate keeps rate × lifetime alive; disabling stops births
    /// and the living ones die out; time scale 0 freezes them.
    #[test]
    fn rate_lifetime_and_enabled() {
        let mut e = Emitter {
            position: Vec3::ZERO,
            rotation: glam::Mat3::IDENTITY,
            enabled: true,
            time_scale: 1.0,
            look: look(20.0),
        };
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

    #[test]
    fn curves() {
        let c = Curve(vec![(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)]);
        assert_eq!(c.at(0.25), 0.5);
        assert_eq!(c.at(0.75), 0.5);
        assert_eq!(c.at(2.0), 0.0);
    }
}
