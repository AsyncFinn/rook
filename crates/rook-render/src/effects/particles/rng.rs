//! The particles' small seeded random number generator.

use super::*;

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

    pub(super) fn between(&mut self, [a, b]: [f32; 2]) -> f32 {
        a + (b - a) * self.next()
    }

    /// A number in [-1, 1).
    pub(super) fn signed(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }

    /// A point in the unit ball, evenly.
    pub(super) fn in_ball(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(self.signed(), self.signed(), self.signed());
            if v.length_squared() <= 1.0 {
                return v;
            }
        }
    }

    /// A point on the unit sphere, evenly.
    pub(super) fn on_sphere(&mut self) -> Vec3 {
        loop {
            let v = self.in_ball();
            if v.length_squared() > 1e-6 {
                return v.normalize();
            }
        }
    }
}
