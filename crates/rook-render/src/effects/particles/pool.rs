//! One emitter's pool of live particles: stepped, born at its rate and
//! kept when the emitter is reread.

use super::*;

#[derive(Debug, Default)]
pub(super) struct Pool {
    pub(super) particles: Vec<Particle>,
    /// Particles owed but not yet born (a fraction of one).
    pub(super) owed: f32,
    pub(super) rng: Rng,
    /// A box around its particles' centers, where they are (along the
    /// emitter's axes when it's locked).
    pub(super) bounds: Option<Aabb>,
    /// The emitter's `emitted` and `cleared` acted on so far.
    pub(super) emitted: u64,
    pub(super) cleared: u64,
}

/// Emitter `new`'s particles: `pool`, which `old` (the same emitter as it
/// was) had, or a fresh one for a new emitter, whose calls made before it
/// was drawn count.
pub(super) fn adopt(old: Option<&Emitter>, mut pool: Pool, new: &Emitter) -> Pool {
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
    pub(super) fn step(&mut self, emitter: &Emitter, dt: f32, rate: f32) -> usize {
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
    pub(super) fn birth(&mut self, emitter: &Emitter, age: f32) {
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
    pub(super) fn region(&self, emitter: &Emitter) -> Aabb {
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
    pub(super) fn acceleration(&self, emitter: &Emitter) -> Vec3 {
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
    pub(super) fn respace(&mut self, old: &Emitter, new: &Emitter) {
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

pub(super) fn point(p: Vec3) -> Aabb {
    Aabb { min: p, max: p }
}
