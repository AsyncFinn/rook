//! Rook's own pseudo-random numbers fixed by position, for what Studio
//! randomizes with a hash Rook doesn't know (the rough materials' shift,
//! where grass blades grow).

use glam::IVec3;

/// A stream of numbers from 0 to 1, fixed by its seed.
pub(crate) struct Random(u32);

impl Random {
    /// The stream for a grid position.
    pub(crate) fn at(p: IVec3) -> Self {
        Self(
            (p.x as u32).wrapping_mul(0x9e37_79b1)
                ^ (p.y as u32).wrapping_mul(0x85eb_ca77)
                ^ (p.z as u32).wrapping_mul(0xc2b2_ae3d),
        )
    }

    pub(crate) fn next(&mut self) -> f32 {
        let h = &mut self.0;
        *h ^= *h >> 15;
        *h = h.wrapping_mul(0x2c1b_3c6d);
        *h ^= *h >> 12;
        *h = h.wrapping_mul(0x297a_2d39);
        *h ^= *h >> 15;
        (*h >> 8) as f32 / (1u32 << 24) as f32
    }
}
