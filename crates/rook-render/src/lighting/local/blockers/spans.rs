//! Boxes of voxels: cutting them, splitting them into pieces, and where
//! each voxel and column lives in the volume's storage.

use super::*;

/// The voxels of `span` from `lo` to `hi`, or `None` if they miss it.
pub(in crate::lighting::local) fn clip((lo, hi): Span, span: Span) -> Option<Span> {
    let (lo, hi) = (lo.max(span.0), hi.min(span.1));
    (!lo.cmpgt(hi).any()).then_some((lo, hi))
}

/// The voxels of `span` outside `cut`, as up to six boxes: those past it
/// along x, then along y among the rest, then along z.
pub(in crate::lighting::local) fn minus(span: Span, cut: Option<Span>) -> Vec<Span> {
    let Some(cut) = cut.and_then(|cut| clip(cut, span)) else {
        return vec![span];
    };
    let mut pieces = Vec::new();
    let mut rest = span;
    for axis in 0..3 {
        let (lo, hi) = (rest.0[axis], rest.1[axis]);
        for (from, to) in [(lo, cut.0[axis] - 1), (cut.1[axis] + 1, hi)] {
            if from <= to {
                let mut piece = rest;
                piece.0[axis] = from;
                piece.1[axis] = to;
                pieces.push(piece);
            }
        }
        rest.0[axis] = cut.0[axis];
        rest.1[axis] = cut.1[axis];
    }
    pieces
}

/// Where a grid keeps a voxel: its slot (`Grid`'s, [`slot_of`] the
/// world's voxel index modulo `dims`), and its column's ([`column_of`])
/// and layer's, so the per-column layers need no division.
#[derive(Debug, Clone, Copy)]
pub(super) struct Slot {
    pub(super) index: usize,
    pub(super) column: usize,
    pub(super) layer: u32,
}

/// Where a grid of `dims` keeps the voxel at `r` (modulo `dims` already):
/// in the brick of its piece, each brick [`PIECE`] across, the volume's
/// height and contiguous, laid out x + 16 * (z + 16 * y) within, as
/// voxel engines store bricks, so a piece is marked and sent up whole.
/// `shaders/voxels/light_volume.wgsl` (`slot`) keeps them the same way.
pub(super) fn slot_of(dims: IVec3, r: IVec3) -> usize {
    let brick = r.x / PIECE + dims.x / PIECE * (r.z / PIECE);
    let within = r.x % PIECE + PIECE * (r.z % PIECE + PIECE * r.y);
    (brick * PIECE * PIECE * dims.y + within) as usize
}

/// Where a grid of `dims` keeps the column at `x`, `z` (modulo `dims`
/// already): by brick as [`slot_of`] keeps voxels, x + 16 * z within.
pub(in crate::lighting::local) fn column_of(dims: IVec3, x: i32, z: i32) -> usize {
    let brick = x / PIECE + dims.x / PIECE * (z / PIECE);
    (brick * PIECE * PIECE + x % PIECE + PIECE * (z % PIECE)) as usize
}

/// Each voxel of `span` with where a grid of `dims` keeps it, walked a row
/// at a time so the modulo is taken per row, not per voxel.
pub(super) fn each(dims: IVec3, (lo, hi): Span, mut f: impl FnMut(IVec3, Slot)) {
    for y in lo.y..=hi.y {
        let ry = y.rem_euclid(dims.y);
        for z in lo.z..=hi.z {
            let rz = z.rem_euclid(dims.z);
            let mut rx = lo.x.rem_euclid(dims.x);
            let mut index = slot_of(dims, IVec3::new(rx, ry, rz));
            for x in lo.x..=hi.x {
                let slot = Slot {
                    index,
                    column: column_of(dims, rx, rz),
                    layer: ry as u32,
                };
                f(IVec3::new(x, y, z), slot);
                rx += 1;
                if rx == dims.x {
                    rx = 0;
                }
                // Along a brick's row the slots are contiguous; past its
                // edge they start the next brick's.
                index = if rx % PIECE == 0 {
                    slot_of(dims, IVec3::new(rx, ry, rz))
                } else {
                    index + 1
                };
            }
        }
    }
}

/// `span` cut into pieces [`PIECE`] voxels across, on a world-aligned grid.
pub(super) fn pieces((lo, hi): Span) -> impl Iterator<Item = Span> {
    let starts = move |lo: i32, hi: i32| {
        let first = lo.div_euclid(PIECE) * PIECE;
        (first..=hi)
            .step_by(PIECE as usize)
            .map(move |s| (s.max(lo), (s + PIECE - 1).min(hi)))
    };
    starts(lo.z, hi.z).flat_map(move |(z0, z1)| {
        starts(lo.x, hi.x).map(move |(x0, x1)| (IVec3::new(x0, lo.y, z0), IVec3::new(x1, hi.y, z1)))
    })
}

/// Voxel `k`'s center.
pub(in crate::lighting::local) fn center(k: IVec3) -> Vec3 {
    k.as_vec3() * VOXEL + VOXEL * 0.5
}
