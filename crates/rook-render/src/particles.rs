//! The particles of the scene's emitters, simulated on the CPU in world
//! space: each emitter's are born at its rate (and when scripts call
//! `Emit`) where it is, move by their velocity, acceleration and drag, and
//! die at the end of their lifetime (docs/behavior/effects.md,
//! docs/behavior/particle-emitters.md).

mod spawn;

use std::sync::Arc;

use rustc_hash::FxHashMap as HashMap;

use glam::{Mat3, Vec3};

use crate::batch::cull::Aabb;
use crate::scene::{Emitter, EmitterLook, FlipbookMode};

/// The longest step simulated at once; longer ones are split, up to
/// [`MAX_STEP`] in all.
const SUBSTEP: f32 = 1.0 / 30.0;
/// The most time one call simulates (a frame after a long stall).
const MAX_STEP: f32 = 1.0;
/// The most particles one `Emit` call births (scripts asking for millions
/// would stall the frame).
const MAX_BURST: u64 = 10_000;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Particle {
    pub(crate) position: Vec3,
    pub(crate) velocity: Vec3,
    age: f32,
    lifetime: f32,
    /// The random factor on its size.
    scale: f32,
    /// Degrees, positive clockwise.
    rotation: f32,
    spin: f32,
    /// Picks its envelopes and random flipbook frames.
    seed: u32,
    /// Flipbook frames per second, and the frame it started at.
    frame_rate: f32,
    first_frame: f32,
}

impl Particle {
    /// Its share of its lifetime gone, 0 to 1.
    pub(crate) fn life(&self) -> f32 {
        (self.age / self.lifetime).clamp(0.0, 1.0)
    }

    /// Its width and height in studs now.
    pub(crate) fn size(&self, look: &EmitterLook) -> [f32; 2] {
        let t = self.life();
        let size = look.size.at(t, |k| self.pick(k)) * self.scale;
        let squash = look.squash.at(t).exp2();
        [size / squash, size * squash]
    }

    /// Its opacity now.
    pub(crate) fn alpha(&self, look: &EmitterLook) -> f32 {
        // The envelope picks differ from the size's.
        look.alpha
            .at(self.life(), |k| self.pick(k + 64))
            .clamp(0.0, 1.0)
    }

    pub(crate) fn rotation(&self) -> f32 {
        self.rotation
    }

    /// The flipbook frame it shows now, of `frames`.
    pub(crate) fn frame(&self, mode: FlipbookMode, frames: u32) -> u32 {
        let n = frames.max(1);
        if mode == FlipbookMode::OneShot {
            return ((self.life() * n as f32) as u32).min(n - 1);
        }
        let k = (self.first_frame + self.age * self.frame_rate).max(0.0) as u32;
        match mode {
            FlipbookMode::PingPong if n > 1 => {
                let k = k % (2 * n - 2);
                if k < n { k } else { 2 * n - 2 - k }
            }
            FlipbookMode::Random => hash(self.seed ^ k.wrapping_mul(0x9E37_79B9)) % n,
            _ => k % n,
        }
    }

    /// A number in [-1, 1] for keypoint `k`, the same every frame.
    fn pick(&self, k: usize) -> f32 {
        let h = hash(self.seed.wrapping_add((k as u32).wrapping_mul(0x85EB_CA6B)));
        (h >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }
}

fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
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
    /// The emitter's `emitted` and `cleared` acted on so far.
    emitted: u64,
    cleared: u64,
}

/// Every emitter's particles, in the scene's order of its emitters.
#[derive(Debug, Default)]
pub(crate) struct Particles {
    emitters: Vec<Emitter>,
    pools: Vec<Pool>,
}

impl Particles {
    /// Take the scene's emitters, acting on scripts' `Emit` and `Clear`
    /// and moving locked particles with their emitter. Particles stay with
    /// their emitter's `key` wherever it moves in the list, so effects
    /// coming and going leave the others' particles alone.
    pub(crate) fn set_emitters(&mut self, emitters: &[Emitter]) {
        let mut old: HashMap<u64, (Emitter, Pool)> = self
            .emitters
            .drain(..)
            .zip(self.pools.drain(..))
            .map(|(e, p)| (e.key, (e, p)))
            .collect();
        // Emitters gone: their particles go with them.
        for new in emitters {
            let pool = match old.remove(&new.key) {
                Some((old, mut pool)) => {
                    if new.look.locked {
                        pool.follow(&old, new);
                    }
                    pool
                }
                // A new emitter: calls made before it was drawn count.
                None => Pool {
                    rng: Rng::new(new.key),
                    ..Default::default()
                },
            };
            self.pools.push(pool);
            let pool = self.pools.last_mut().expect("just pushed");
            if new.cleared != pool.cleared {
                pool.particles.clear();
                pool.cleared = new.cleared;
            }
            let burst = new.emitted.saturating_sub(pool.emitted).min(MAX_BURST);
            pool.emitted = new.emitted;
            for _ in 0..burst {
                pool.birth(new, 0.0);
            }
        }
        self.emitters.extend_from_slice(emitters);
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
        let acceleration = acceleration(emitter);
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
            self.birth(emitter, age);
        }
    }

    /// A new particle, `age` seconds old.
    fn birth(&mut self, emitter: &Emitter, age: f32) {
        let look = &*emitter.look;
        let r = &mut self.rng;
        let lifetime = r.between(look.lifetime);
        // A lifetime of 0 emits nothing (creator-docs).
        if lifetime <= 0.0 {
            return;
        }
        let (offset, velocity) = spawn::spawn(look, r);
        let rotation = r.between(look.rotation);
        let mut spin = r.between(look.spin);
        if look.spin_either_way && r.next() < 0.5 {
            spin = -spin;
        }
        let (frame_rate, first_frame) = match look.flipbook {
            Some(f) => {
                let start = if f.start_random {
                    r.next() * f.frames() as f32
                } else {
                    0.0
                };
                (r.between(f.framerate), start)
            }
            None => (0.0, 0.0),
        };
        let seed = (r.next() * 16_777_216.0) as u32;
        let mut p = Particle {
            position: emitter.position + emitter.rotation * offset,
            velocity: emitter.rotation * velocity,
            age,
            lifetime,
            scale: r.between(look.size_spread),
            rotation,
            spin,
            seed,
            frame_rate,
            first_frame,
        };
        let acceleration = acceleration(emitter);
        p.velocity += acceleration * age;
        p.position += p.velocity * age;
        self.particles.push(p);
    }

    /// Carry the particles of a locked emitter from where it was to where
    /// it is.
    fn follow(&mut self, old: &Emitter, new: &Emitter) {
        if old.position == new.position && old.rotation == new.rotation {
            return;
        }
        let turn: Mat3 = new.rotation * old.rotation.transpose();
        for p in &mut self.particles {
            p.position = new.position + turn * (p.position - old.position);
            p.velocity = turn * p.velocity;
        }
    }

    /// A box around its particles (their centres).
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

/// An emitter's acceleration in the world.
fn acceleration(emitter: &Emitter) -> Vec3 {
    emitter.rotation * emitter.look.acceleration + emitter.look.world_acceleration
}

/// A small, fast random number generator (SplitMix64), seeded per
/// emitter so that the same scene draws the same particles (and per part,
/// for where its material textures start).
#[derive(Debug, Clone, Default)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x2545_F491_4F6C_DD1D)
    }

    /// A number in [0, 1).
    pub(crate) fn next(&mut self) -> f32 {
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

    /// A number in [-1, 1).
    fn signed(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }

    /// A point in the unit ball, evenly.
    fn in_ball(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(self.signed(), self.signed(), self.signed());
            if v.length_squared() <= 1.0 {
                return v;
            }
        }
    }

    /// A point on the unit sphere, evenly.
    fn on_sphere(&mut self) -> Vec3 {
        loop {
            let v = self.in_ball();
            if v.length_squared() > 1e-6 {
                return v.normalize();
            }
        }
    }
}

#[cfg(test)]
mod tests;
