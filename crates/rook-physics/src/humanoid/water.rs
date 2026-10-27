//! Terrain water: whether a root part's center is in it, and how high its
//! surface is above it.

use super::*;

/// Water deeper than this many voxels above the root counts as the
/// surface being out of reach (the search stops there).
const MAX_DEPTH_VOXELS: i32 = 64;

impl Physics {
    /// The height of the water's surface over `point`, when `point` is in
    /// terrain water.
    pub(super) fn water_surface(&self, point: glam::Vec3) -> Option<f32> {
        self.terrain.instance?;
        let grid = self.terrain.grid();
        let at = (point / rook_terrain::VOXEL_STUDS).floor().as_ivec3();
        let fill = |v: rook_terrain::Voxel| match v.material {
            rook_terrain::TerrainMaterial::Water => v.fill(),
            _ if v.water > 0 => f32::from(v.water) / 255.0,
            _ => 0.0,
        };
        let here = fill(grid.voxel(at));
        let bottom = at.y as f32 * rook_terrain::VOXEL_STUDS;
        if here <= 0.0 || point.y > bottom + here * rook_terrain::VOXEL_STUDS {
            return None;
        }
        let mut top = at;
        let mut top_fill = here;
        for _ in 0..MAX_DEPTH_VOXELS {
            let above = top + glam::IVec3::Y;
            let f = fill(grid.voxel(above));
            if f <= 0.0 {
                break;
            }
            top = above;
            top_fill = f;
        }
        Some((top.y as f32 + top_fill) * rook_terrain::VOXEL_STUDS)
    }
}
