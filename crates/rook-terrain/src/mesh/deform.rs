//! How a material moves its vertices from where the edge crossings put
//! them (zeux, "Roblox: Large-scale terrain", SIGGRAPH 2020: shift,
//! cubify, quantize), by Studio's material tables ([`Look`]), measured in
//! Studio (`docs/behavior/terrain/surface.md`).

use glam::{IVec3, Vec3};

use crate::material::Look;
use crate::random::Random;

/// Move vertex `p` of the cell whose first corner (the center of its first
/// voxel) is `first` and whose first voxel is `cell` in the grid, its
/// voxels `scale` studs on a side (coarser far away: the moves scale with
/// the cell, so a far Brick ball is still boxes, of the coarse cells).
pub(super) fn deform(p: Vec3, first: Vec3, scale: f32, look: &Look, cell: IVec3) -> Vec3 {
    let center = first + Vec3::splat(scale / 2.0);
    let mut p = p;
    if look.shift > 0.0 {
        // Studio offsets each vertex pseudo-randomly, fixed by position;
        // its hash isn't known, so Rook's own picks the offsets, up to the
        // material's shift either way along each axis, in half-cells
        // (Approximation: Rock in Studio moved by up to about a stud).
        p += (shift(cell) * 2.0 - 1.0) * look.shift * scale / 2.0;
    }
    if look.cubify > 0.0 {
        // Towards the cell's center: all the way for Brick and WoodPlanks,
        // whose lone voxel is its whole 4-stud box at any occupancy.
        p = p.lerp(center, look.cubify);
    }
    if look.quantize > 0.0 {
        p = quantize(p, center, look.quantize * scale);
    }
    p
}

/// Round `p` to multiples of `step` studs from the cell's center, halves
/// away from it (measured: a half-full Concrete slab's edge comes in to
/// the voxel centers, not out; a lone Cobblestone voxel, at steps of 0.6,
/// reaches 1.40 studs instead of 1.33).
fn quantize(p: Vec3, center: Vec3, step: f32) -> Vec3 {
    center + ((p - center) / step).round() * step
}

/// Three numbers from 0 to 1 fixed by a cell's position.
fn shift(cell: IVec3) -> Vec3 {
    let mut r = Random::at(cell);
    Vec3::new(r.next(), r.next(), r.next())
}
