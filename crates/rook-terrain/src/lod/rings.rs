//! The rings each level of detail is drawn in around the camera, and when
//! they move (docs/behavior/terrain/detail.md#how-rook-does-it).

use super::*;

/// Where each level's square is, but the last's: its first chunk across X
/// and Z, in that level's chunks (an even one, a corner of the next
/// level's chunks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rings {
    pub(super) starts: [IVec2; LAST as usize],
}

impl Rings {
    pub fn around(camera: Vec3) -> Self {
        let mut rings = Rings {
            starts: [IVec2::ZERO; LAST as usize],
        };
        rings.place(camera, true);
        rings
    }

    /// Move the squares the camera has left. Returns whether any moved.
    pub fn follow(&mut self, camera: Vec3) -> bool {
        let old = *self;
        self.place(camera, false);
        *self != old
    }

    fn place(&mut self, camera: Vec3, fresh: bool) {
        // Level 0 follows the camera; each after holds the one inside.
        let at = glam::Vec2::new(camera.x, camera.z) / chunk_studs(0);
        let half = (width(0) / 2) as f32;
        let center = self.starts[0].as_vec2() + half;
        if fresh || (at - center).abs().max_element() > 1.0 + SLACK {
            self.starts[0] = even((at - half) / 2.0);
        }
        for k in 1..LAST as usize {
            let at = glam::Vec2::new(camera.x, camera.z) / chunk_studs(k as u8);
            let inner = self.starts[k - 1] / 2;
            let reach = width(k - 1) / 2;
            // Starts that leave a chunk of this level each side of the one
            // inside: from `inner + reach + 1 - width` to `inner - 1`.
            let lo = inner + reach + 1 - width(k);
            let hi = inner - 1;
            let fits = |s: IVec2| s.cmpge(lo).all() && s.cmple(hi).all();
            if fresh || !fits(self.starts[k]) {
                let want = even((at - (width(k) / 2) as f32) / 2.0);
                let lo = IVec2::new(round_up_even(lo.x), round_up_even(lo.y));
                let hi = IVec2::new(round_down_even(hi.x), round_down_even(hi.y));
                self.starts[k] = want.clamp(lo, hi);
            }
        }
    }

    pub fn draws(&self, level: u8, chunk: IVec3) -> bool {
        let column = IVec2::new(chunk.x, chunk.z);
        let within = |lo: IVec2, hi: IVec2| column.cmpge(lo).all() && column.cmplt(hi).all();
        let k = usize::from(level);
        let outer = level == LAST || within(self.starts[k], self.starts[k] + width(k));
        let inner = level > 0 && {
            let lo = self.starts[k - 1] / 2;
            within(lo, lo + width(k - 1) / 2)
        };
        outer && !inner
    }

    /// The sides of chunk `chunk`, drawn at `level`, that face the next
    /// level's ring: those the chunk closes against it
    /// (`crate::mesh::mesh_drawn_chunk`). Each square holds the one inside
    /// with a chunk of its own on every side, so a chunk's neighbor across
    /// X or Z is drawn at its level, the next, or the one before, whose
    /// chunk closes that side.
    pub fn seams(&self, level: u8, chunk: IVec3) -> Sides {
        if level == LAST {
            return Sides::NONE;
        }
        let k = usize::from(level);
        let (lo, hi) = (self.starts[k], self.starts[k] + width(k));
        let mut sides = Sides::NONE;
        for (i, dir) in Sides::DIRECTIONS.iter().enumerate() {
            let column = IVec2::new(chunk.x + dir.x, chunk.z + dir.z);
            if !(column.cmpge(lo).all() && column.cmplt(hi).all()) {
                sides = sides.with(i);
            }
        }
        sides
    }

    /// The chunks of `level` drawn at it that can have a surface, `grid`
    /// holding that level's voxels and `rows` the lowest and highest
    /// level-0 chunk rows that can, in chunk order.
    pub fn chunks(&self, level: u8, grid: &Grid, rows: (i32, i32)) -> Vec<IVec3> {
        if level == LAST {
            return chunks_to_mesh(grid)
                .into_iter()
                .filter(|c| self.draws(level, *c))
                .collect();
        }
        let first = self.starts[usize::from(level)];
        let side = width(usize::from(level));
        let step = 1 << level;
        let (lo, hi) = (rows.0.div_euclid(step) - 1, rows.1.div_euclid(step));
        let mut out = Vec::new();
        for x in 0..side {
            for y in lo..=hi {
                for z in 0..side {
                    let c = IVec3::new(first.x + x, y, first.y + z);
                    if self.draws(level, c) && can_have_surface(grid, c) {
                        out.push(c);
                    }
                }
            }
        }
        out
    }
}

/// The even point nearest `v` (in chunks), times 2: a corner of the next
/// level's chunks.
fn even(v: glam::Vec2) -> IVec2 {
    v.round().as_ivec2() * 2
}

fn round_up_even(v: i32) -> i32 {
    v + v.rem_euclid(2)
}

fn round_down_even(v: i32) -> i32 {
    v - v.rem_euclid(2)
}
