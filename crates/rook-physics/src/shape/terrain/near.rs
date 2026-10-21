//! Which terrain chunks a moving part or a query may reach, so their
//! colliders are built before Rapier looks for them.

use glam::{IVec3, Vec3};

use super::*;
use crate::convert::to_glam;

/// How far a chunk's surface reaches outside its own box, rounded up: the
/// quads of edges into the next chunk reach the next voxels' centres (6
/// studs), the rough materials shift vertices by about a stud, and water
/// sits 2 lower.
const REACH: f32 = 8.0;

const CHUNK_STUDS: f32 = CHUNK_VOXELS as f32 * VOXEL_STUDS;

/// Rays are walked in pieces this long, each piece's box finding chunks.
const STEP: f32 = 32.0;

impl Terrain {
    /// Unbuilt chunks whose surface may reach the box from `min` to `max`.
    fn unbuilt_in(&self, min: Vec3, max: Vec3, out: &mut Vec<IVec3>) {
        let lo = ((min - REACH) / CHUNK_STUDS).floor().as_ivec3();
        let hi = ((max + REACH) / CHUNK_STUDS).floor().as_ivec3();
        let span = (hi - lo + IVec3::ONE).max(IVec3::ZERO).as_dvec3();
        if span.x * span.y * span.z > self.unbuilt.len() as f64 {
            out.extend(
                self.unbuilt
                    .iter()
                    .filter(|c| c.cmpge(lo).all() && c.cmple(hi).all()),
            );
            return;
        }
        for z in lo.z..=hi.z {
            for y in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    let c = IVec3::new(x, y, z);
                    if self.unbuilt.contains(&c) {
                        out.push(c);
                    }
                }
            }
        }
    }

    /// The part of a ray from `origin` along unit `dir` for `length` studs
    /// that passes near the terrain's chunks: its start and end distances.
    fn clip(&self, origin: Vec3, dir: Vec3, length: f32) -> Option<(f32, f32)> {
        let (lo, hi) = self.extent?;
        let lo = lo.as_vec3() * CHUNK_STUDS - REACH;
        let hi = (hi + IVec3::ONE).as_vec3() * CHUNK_STUDS + REACH;
        let (mut t0, mut t1) = (0.0f32, length);
        for axis in 0..3 {
            let (o, d) = (origin[axis], dir[axis]);
            if d.abs() < 1e-12 {
                if o < lo[axis] || o > hi[axis] {
                    return None;
                }
                continue;
            }
            let (a, b) = ((lo[axis] - o) / d, (hi[axis] - o) / d);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
        (t0 <= t1).then_some((t0, t1))
    }
}

impl Physics {
    /// Build the chunks whose surface may reach the box from `min` to
    /// `max`.
    pub(crate) fn build_terrain_in(&mut self, min: Vec3, max: Vec3) {
        if self.terrain.unbuilt.is_empty() {
            return;
        }
        let mut want = Vec::new();
        self.terrain.unbuilt_in(min, max, &mut want);
        self.build_wanted(want);
    }

    /// Build the chunks a ray from `origin` along unit `dir` may hit in
    /// `length` studs.
    pub(crate) fn build_terrain_along(&mut self, origin: Vec3, dir: Vec3, length: f32) {
        if self.terrain.unbuilt.is_empty() {
            return;
        }
        let Some((mut t, end)) = self.terrain.clip(origin, dir, length) else {
            return;
        };
        let mut want = Vec::new();
        while t < end {
            let next = (t + STEP).min(end);
            let (a, b) = (origin + dir * t, origin + dir * next);
            self.terrain.unbuilt_in(a.min(b), a.max(b), &mut want);
            t = next;
        }
        self.build_wanted(want);
    }

    /// Before a step of `dt` seconds: build the chunks every moving body
    /// may reach in it.
    pub(crate) fn build_terrain_near_bodies(&mut self, dt: f32) {
        if self.terrain.unbuilt.is_empty() {
            return;
        }
        let mut want = Vec::new();
        // Bodies added or moved since the last step aren't active yet.
        let moving = self
            .world
            .islands
            .active_bodies()
            .chain(self.stale.iter().copied());
        for h in moving {
            let Some(rb) = self.world.bodies.get(h) else {
                continue;
            };
            if !rb.is_dynamic() {
                continue;
            }
            let reach = to_glam(rb.linvel()).length() * dt;
            for c in rb.colliders() {
                let co = &self.world.colliders[*c];
                let local = co.position_wrt_parent().copied().unwrap_or_default();
                let aabb = co.shape().compute_aabb(&(rb.position() * local));
                self.terrain.unbuilt_in(
                    to_glam(aabb.mins) - reach,
                    to_glam(aabb.maxs) + reach,
                    &mut want,
                );
            }
        }
        self.build_wanted(want);
    }

    fn build_wanted(&mut self, mut want: Vec<IVec3>) {
        if want.is_empty() {
            return;
        }
        want.sort_unstable_by_key(|c| (c.x, c.y, c.z));
        want.dedup();
        self.build_chunks(&want);
    }
}
