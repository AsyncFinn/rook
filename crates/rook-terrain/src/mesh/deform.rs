//! How a material moves its vertices from where the edge crossings put
//! them (zeux, "Roblox: Large-scale terrain", SIGGRAPH 2020: shift,
//! cubify, quantize), by Studio's material tables ([`Look`]), measured in
//! Studio (`docs/behavior/terrain.md`).

use glam::{IVec3, Vec3};

use crate::Look;
use crate::grid::VOXEL_STUDS;

/// Move vertex `p` of the cell whose first corner (the centre of its first
/// voxel) is `first` and whose first voxel is `cell` in the grid.
pub(super) fn deform(p: Vec3, first: Vec3, look: &Look, cell: IVec3) -> Vec3 {
    let centre = first + Vec3::splat(VOXEL_STUDS / 2.0);
    let mut p = p;
    if look.shift > 0.0 {
        // Studio offsets each vertex pseudo-randomly, fixed by position;
        // its hash isn't known, so Rook's own picks the offsets, up to the
        // material's shift either way along each axis, in half-cells
        // (Approximation: Rock in Studio moved by up to about a stud).
        p += (shift(cell) * 2.0 - 1.0) * look.shift * VOXEL_STUDS / 2.0;
    }
    if look.cubify > 0.0 {
        // Towards the cell's centre: all the way for Brick and WoodPlanks,
        // whose lone voxel is its whole 4-stud box at any occupancy.
        p = p.lerp(centre, look.cubify);
    }
    if look.quantize > 0.0 {
        p = quantize(p, centre, look.quantize * VOXEL_STUDS);
    }
    p
}

/// Round `p` to multiples of `step` studs from the cell's centre, halves
/// away from it (measured: a half-full Concrete slab's edge comes in to
/// the voxel centres, not out; a lone Cobblestone voxel, at steps of 0.6,
/// reaches 1.40 studs instead of 1.33).
fn quantize(p: Vec3, centre: Vec3, step: f32) -> Vec3 {
    centre + ((p - centre) / step).round() * step
}

/// Three numbers from 0 to 1 fixed by a cell's position.
fn shift(cell: IVec3) -> Vec3 {
    let mut h = (cell.x as u32).wrapping_mul(0x9e37_79b1)
        ^ (cell.y as u32).wrapping_mul(0x85eb_ca77)
        ^ (cell.z as u32).wrapping_mul(0xc2b2_ae3d);
    let mut next = || {
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        h = h.wrapping_mul(0x297a_2d39);
        h ^= h >> 15;
        (h >> 8) as f32 / (1u32 << 24) as f32
    };
    Vec3::new(next(), next(), next())
}
