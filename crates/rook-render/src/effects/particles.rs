//! The particles of the scene's emitters, simulated on the CPU as Studio
//! simulates them (docs/behavior/effects/fire-smoke-and-sparkles.md,
//! docs/behavior/effects/particle-emitters.md): only emitters the camera
//! can see move on. Each frame, at draw time, an emitter whose particles
//! (or the place it births them) reach into the view steps once by the
//! frame's time, births at its rate (thinned with its distance from the
//! camera) and lets particles die at the end of their lifetimes. One out
//! of view is left as it is, its time stopped, until the camera turns
//! back to it. Scripts' `Emit` births at once, wherever the emitter is.

mod spawn;

use rustc_hash::FxHashMap;

use glam::{Mat3, Vec3};

use crate::batch::cull::{Aabb, Frustum};
use crate::scene::{Emitter, EmitterLook, FlipbookMode};

/// The most time one frame moves an emitter on (Studio: frames of 0.1 to
/// 0.18 s each moved particles 1/15 s).
const MAX_FRAME: f32 = 1.0 / 15.0;
/// An emitter's `Rate` thins with its distance from the camera: all of
/// it within this many studs...
const RATE_FULL_STUDS: f32 = 150.0;
/// ...falling evenly to none at this many (Studio: 0.82 of it at 300
/// studs, 0.59 at 500, 0.12 at 900).
const RATE_NONE_STUDS: f32 = 1000.0;
/// The most particles a frame draws: Studio draws whole emitters, nearest
/// the camera first, while their particles add up to at most this many.
pub(crate) const DRAWN_MOST: usize = 16_000;
/// The most particles one `Emit` call births (scripts asking for millions
/// would stall the frame).
const MAX_BURST: u64 = 10_000;

/// Up to this many particles to step, a frame steps them on its own
/// thread: waking the workers costs about as much as stepping this many
/// (as `rook-platform`'s sort keeps small sorts on one thread).
const STEP_ALONE: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Particle {
    /// In the world, or for a locked emitter along its axes from it (its
    /// particles move with it at no cost until they're drawn).
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

    /// Move on by `dt` seconds under `acceleration`, its velocity then
    /// multiplied by `damping`. Studio's step, fitted to its particles at
    /// 15 frames a second: the position goes on by the velocity and
    /// acceleration over the step, then the velocity takes the
    /// acceleration and loses its drag.
    fn advance(&mut self, dt: f32, acceleration: Vec3, damping: f32) {
        self.age += dt;
        self.position += self.velocity * dt + acceleration * (0.5 * dt * dt);
        self.velocity = (self.velocity + acceleration * dt) * damping;
        self.rotation += self.spin * dt;
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
    /// A box around its particles' centers, where they are (along the
    /// emitter's axes when it's locked).
    bounds: Option<Aabb>,
    /// The emitter's `emitted` and `cleared` acted on so far.
    emitted: u64,
    cleared: u64,
}

/// Every emitter's particles, by the scene's emitter slots.
#[derive(Debug, Default)]
pub(crate) struct Particles {
    emitters: Vec<Option<Emitter>>,
    /// Empty for a free slot.
    pools: Vec<Pool>,
    /// The time passed since the last frame, which the next one moves
    /// what it sees on by.
    pending: f32,
}

/// An emitter in view, moved on this frame by `dt`.
struct Step<'a> {
    emitter: &'a Emitter,
    pool: &'a mut Pool,
    dt: f32,
    /// The share of its rate it births at, by its distance.
    rate: f32,
    /// Particles moved on.
    stepped: usize,
}

impl Particles {
    /// Take a new scene's emitters. Particles stay with their emitter's
    /// `key` wherever it is now, so a scene built again (a MaterialVariant
    /// changed) leaves them alone.
    pub(crate) fn replace(&mut self, emitters: &[Option<Emitter>]) {
        let mut old: FxHashMap<u64, (Emitter, Pool)> = (self.emitters.drain(..))
            .zip(self.pools.drain(..))
            .filter_map(|(e, p)| e.map(|e| (e.key, (e, p))))
            .collect();
        for new in emitters {
            let pool = match new {
                Some(new) => match old.remove(&new.key) {
                    Some((old, pool)) => adopt(Some(&old), pool, new),
                    None => adopt(None, Pool::default(), new),
                },
                None => Pool::default(),
            };
            self.pools.push(pool);
        }
        self.emitters.extend_from_slice(emitters);
    }

    /// Take the emitter now in slot `i` (`None`: it's gone, and its
    /// particles with it), acting on scripts' `Emit` and `Clear`. Another
    /// emitter taking the slot starts with none. Moving costs nothing
    /// here: born particles stay put, and a locked emitter's are kept
    /// along its axes.
    pub(crate) fn set(&mut self, i: usize, new: Option<&Emitter>) {
        if self.emitters.len() <= i {
            self.emitters.resize(i + 1, None);
            self.pools.resize_with(i + 1, Pool::default);
        }
        let pool = std::mem::take(&mut self.pools[i]);
        let old = self.emitters[i].take();
        self.pools[i] = match new {
            Some(new) => match old.filter(|old| old.key == new.key) {
                Some(old) => adopt(Some(&old), pool, new),
                None => adopt(None, Pool::default(), new),
            },
            None => Pool::default(),
        };
        self.emitters[i] = new.cloned();
    }

    /// Let `dt` seconds pass: the next [`frame`](Self::frame) moves what
    /// it sees on by the time passed since the one before.
    pub(crate) fn advance(&mut self, dt: f32) {
        self.pending += dt.max(0.0);
    }

    /// A frame seen from `camera` through `view`: every emitter whose
    /// particles, or the place it births them, reach into the view moves
    /// on by the time passed (each its `time_scale` of it, at most
    /// [`MAX_FRAME`]); the rest stay exactly as they are. Returns the
    /// slots in view.
    pub(crate) fn frame(&mut self, view: &Frustum, camera: Vec3) -> Vec<usize> {
        profiling::scope!("Particles::frame");
        let dt = std::mem::take(&mut self.pending).min(MAX_FRAME);
        let mut seen = Vec::new();
        let mut steps: Vec<Step<'_>> = Vec::new();
        let mut alive = 0;
        for (i, (emitter, pool)) in self.emitters.iter().zip(&mut self.pools).enumerate() {
            alive += pool.particles.len();
            let Some(emitter) = emitter else {
                continue;
            };
            let look = &*emitter.look;
            if !view.sees(pool.region(emitter), reach(look) + look.z_offset.abs()) {
                continue;
            }
            seen.push(i);
            let dt = dt * emitter.time_scale.max(0.0);
            if dt > 0.0 {
                steps.push(Step {
                    emitter,
                    pool,
                    dt,
                    rate: rate_share(camera.distance(emitter.position)),
                    stepped: 0,
                });
            }
        }
        // Each emitter steps on its own, so in parallel when there's
        // enough to step.
        let step = |_, chunk: &mut [Step<'_>]| {
            for s in chunk {
                s.stepped = s.pool.step(s.emitter, s.dt, s.rate);
            }
        };
        let work: usize = steps.iter().map(|s| s.pool.particles.len()).sum();
        if work <= STEP_ALONE {
            step(0, &mut steps);
        } else {
            rook_platform::get()
                .workers
                .for_each_chunk(&mut steps, 1, step);
        }
        rook_types::count!(
            ParticlesStepped,
            steps.iter().map(|s| s.stepped).sum::<usize>()
        );
        rook_types::gauge!(Particles, alive);
        seen
    }

    /// Of the emitters at `slots`, those Studio would draw: nearest
    /// `camera` first, each whole while their particles add up to at most
    /// [`DRAWN_MOST`]; one that doesn't fit is left out, and farther ones
    /// still may (Studio: three 5,000-particle emitters, then ten
    /// 100-particle ones past a fourth). Kept in that order.
    pub(crate) fn within_budget(&self, slots: &mut Vec<usize>, camera: Vec3) {
        let distance = |i: usize| {
            self.emitters[i]
                .as_ref()
                .map_or(0.0, |e| e.position.distance_squared(camera))
        };
        slots.sort_by(|&a, &b| distance(a).total_cmp(&distance(b)).then(a.cmp(&b)));
        let mut room = DRAWN_MOST;
        slots.retain(|&i| {
            let n = self.pools[i].particles.len();
            let fits = n <= room;
            if fits {
                room -= n;
            }
            fits
        });
    }

    /// The emitter in slot `i` and its particles.
    pub(crate) fn emitter(&self, i: usize) -> Option<(&Emitter, &[Particle])> {
        Some((self.emitters.get(i)?.as_ref()?, &self.pools[i].particles))
    }

    /// Whether any emitter is in the scene.
    pub(crate) fn is_empty(&self) -> bool {
        self.emitters.iter().all(Option::is_none)
    }
}

/// The share of its `Rate` an emitter `distance` studs from the camera
/// births at.
fn rate_share(distance: f32) -> f32 {
    ((RATE_NONE_STUDS - distance) / (RATE_NONE_STUDS - RATE_FULL_STUDS)).clamp(0.0, 1.0)
}

/// How far from its center a particle of `look` may draw: its biggest
/// quad turned any way stays within 0.71 × its size.
pub(crate) fn reach(look: &EmitterLook) -> f32 {
    let squash = look.squash.0.iter().map(|p| p.1.abs()).fold(0.0, f32::max);
    let biggest = look.size.max() * look.size_spread[1].max(look.size_spread[0]) * squash.exp2();
    biggest * 0.75
}

/// `b` moved from along an emitter's axes into the world.
fn placed(b: Aabb, position: Vec3, rotation: Mat3) -> Aabb {
    if b.is_empty() {
        return b;
    }
    let center = position + rotation * ((b.min + b.max) * 0.5);
    let half = (b.max - b.min) * 0.5;
    let abs = Mat3::from_cols(
        rotation.x_axis.abs(),
        rotation.y_axis.abs(),
        rotation.z_axis.abs(),
    );
    let half = abs * half;
    Aabb {
        min: center - half,
        max: center + half,
    }
}

/// Emitter `new`'s particles: `pool`, which `old` (the same emitter as it
/// was) had, or a fresh one for a new emitter, whose calls made before it
/// was drawn count.
fn adopt(old: Option<&Emitter>, mut pool: Pool, new: &Emitter) -> Pool {
    match old {
        Some(old) if old.look.locked != new.look.locked => pool.respace(old, new),
        Some(_) => {}
        None => pool.rng = Rng::new(new.key),
    }
    if new.cleared != pool.cleared {
        pool.particles.clear();
        pool.bounds = None;
        pool.cleared = new.cleared;
    }
    let burst = new.emitted.saturating_sub(pool.emitted).min(MAX_BURST);
    pool.emitted = new.emitted;
    for _ in 0..burst {
        pool.birth(new, 0.0);
    }
    pool
}

impl Pool {
    /// Move every particle on by `dt` seconds, kill those past their
    /// lifetime and birth at the emitter's rate, `rate` of it. Returns the
    /// particles moved.
    fn step(&mut self, emitter: &Emitter, dt: f32, rate: f32) -> usize {
        let look = &*emitter.look;
        let damping = (-look.drag * dt).exp();
        let acceleration = self.acceleration(emitter);
        let stepped = self.particles.len();
        let mut bounds = Aabb::EMPTY;
        self.particles.retain_mut(|p| {
            p.advance(dt, acceleration, damping);
            let alive = p.age < p.lifetime;
            if alive {
                bounds = bounds.union(point(p.position));
            }
            alive
        });
        self.bounds = (!bounds.is_empty()).then_some(bounds);
        if !emitter.enabled || look.rate <= 0.0 {
            self.owed = 0.0;
            return stepped;
        }
        self.owed += look.rate * rate * dt;
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            // Born some way into the step, so a steady rate doesn't
            // come out in layers.
            let age = self.rng.next() * dt;
            self.birth(emitter, age);
        }
        stepped
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
        let (position, velocity) = if look.locked {
            (offset, velocity)
        } else {
            (
                emitter.position + emitter.rotation * offset,
                emitter.rotation * velocity,
            )
        };
        let mut p = Particle {
            position,
            velocity,
            age: 0.0,
            lifetime,
            scale: r.between(look.size_spread),
            rotation,
            spin,
            seed,
            frame_rate,
            first_frame,
        };
        if age > 0.0 {
            p.advance(age, self.acceleration(emitter), (-look.drag * age).exp());
        }
        self.bounds = Some(self.bounds.unwrap_or(Aabb::EMPTY).union(point(p.position)));
        self.particles.push(p);
    }

    /// Where its particles are, and where `emitter` births them, in the
    /// world.
    fn region(&self, emitter: &Emitter) -> Aabb {
        let half = emitter.look.spawn.half();
        let mut local = Aabb {
            min: -half,
            max: half,
        };
        if emitter.look.locked {
            local = local.union(self.bounds.unwrap_or(Aabb::EMPTY));
        }
        let world = placed(local, emitter.position, emitter.rotation);
        match self.bounds {
            Some(b) if !emitter.look.locked => world.union(b),
            _ => world,
        }
    }

    /// The emitter's acceleration where its particles are kept.
    fn acceleration(&self, emitter: &Emitter) -> Vec3 {
        let look = &*emitter.look;
        let world = emitter.rotation * look.acceleration + look.world_acceleration;
        if look.locked {
            emitter.rotation.transpose() * world
        } else {
            world
        }
    }

    /// The emitter turned locked or unlocked: keep its particles where
    /// they are, along `new`'s axes or in the world.
    fn respace(&mut self, old: &Emitter, new: &Emitter) {
        let to_local = new.rotation.transpose();
        for p in &mut self.particles {
            if old.look.locked {
                p.position = old.position + old.rotation * p.position;
                p.velocity = old.rotation * p.velocity;
            }
            if new.look.locked {
                p.position = to_local * (p.position - new.position);
                p.velocity = to_local * p.velocity;
            }
        }
        let bounds = self
            .particles
            .iter()
            .fold(Aabb::EMPTY, |b, p| b.union(point(p.position)));
        self.bounds = (!bounds.is_empty()).then_some(bounds);
    }
}

fn point(p: Vec3) -> Aabb {
    Aabb { min: p, max: p }
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
