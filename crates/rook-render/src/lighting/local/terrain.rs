//! Terrain in the light voxels: a byte per voxel, how full of solid terrain
//! it is, marked beside the parts (`blockers.rs`) for the fill pass to dim
//! the sun and the sky's light by (`soil_sun` and `soil_sky` in
//! `shaders/light_volume.wgsl`). Local lights pass through it, as in
//! Studio. Light voxel `k` is terrain voxel `k`: both span `4k` to `4k + 4`.
//! What Studio does is in docs/behavior/lighting/sun.md and
//! docs/behavior/rendering/materials.md ("The sky's diffuse light").

use rook_terrain::{Grid, VOXEL_STUDS, Voxel, chunk_of};

use super::blockers::Span;
use super::*;

const _: () = assert!(VOXEL_STUDS == VOXEL, "a light voxel is a terrain voxel");

/// The terrain's voxels as the light voxels were last told them, and the
/// boxes of them that changed since they were marked.
#[derive(Debug, Default)]
pub(super) struct Terrain {
    /// Shares the DataModel's chunks until they change (copy on write).
    voxels: Grid,
    edits: Vec<Span>,
}

/// What a [`Terrain::take`] changed.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Taken {
    /// All of it: every voxel is marked afresh.
    Whole,
    /// Only the boxes [`Terrain::take_edits`] gives.
    Edits,
}

impl Terrain {
    /// Take the voxels the scene sends.
    pub(super) fn take(&mut self, update: TerrainVoxels) -> Taken {
        match update {
            TerrainVoxels::Whole(voxels) => {
                self.voxels = voxels;
                self.edits.clear();
                Taken::Whole
            }
            TerrainVoxels::Edits { chunks, bounds } => {
                for b in bounds {
                    self.voxels.put_region(&chunks, b);
                    self.edits.push((b.min, b.max));
                }
                Taken::Edits
            }
        }
    }

    /// Whether a box of voxels changed since they were marked reaches
    /// `span`.
    pub(super) fn reaches(&self, span: Span) -> bool {
        self.edits
            .iter()
            .any(|&edit| super::blockers::clip(edit, span).is_some())
    }

    /// The boxes of voxels that changed since this was last called.
    pub(super) fn take_edits(&mut self) -> Vec<Span> {
        std::mem::take(&mut self.edits)
    }

    /// Each voxel of `span`'s [`soil`], X fastest, then Y, then Z, into
    /// `out`; empty when none of them holds solid terrain.
    pub(super) fn soils(&self, (lo, hi): Span, voxels: &mut Vec<Voxel>, out: &mut Vec<u8>) {
        out.clear();
        let ((first, _), (last, _)) = (chunk_of(lo), chunk_of(hi));
        let solid = (first.z..=last.z).any(|z| {
            (first.y..=last.y).any(|y| {
                (first.x..=last.x).any(|x| {
                    let chunk = self.voxels.uniform(IVec3::new(x, y, z));
                    chunk.is_none_or(Voxel::is_solid)
                })
            })
        });
        if !solid {
            return;
        }
        self.voxels.read_block(lo, hi - lo + 1, voxels);
        out.extend(voxels.iter().map(|&v| soil(v)));
        if out.iter().all(|&s| s == 0) {
            out.clear();
        }
    }
}

/// How full of solid terrain a voxel is, as the fill pass reads it: 0 for
/// air and water (a water roof stops none of the sky's light and casts no
/// shadow in Studio), else its occupancy in 255ths (byte `b` is
/// `(b + 1) / 256` full: at least 1).
pub(super) fn soil(v: Voxel) -> u8 {
    if v.is_solid() {
        v.occupancy.saturating_add(1)
    } else {
        0
    }
}
