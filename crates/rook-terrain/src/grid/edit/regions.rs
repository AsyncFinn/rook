//! Region reads and writes: which voxels a `Region3` covers, and
//! writing, copying and pasting blocks of voxels.

use super::*;

/// The most voxels a region read or write may cover (observed: 4,194,304
/// is allowed, 256 × 256 × 65 is "Region is too large").
pub const REGION_MAX_VOXELS: i64 = 1 << 22;

/// The voxels a `Region3` from `min` to `max` covers for region methods:
/// its corners in voxels, truncated toward zero (observed: -1.9 to 2.1
/// covers no voxel, 1 to 8 covers voxels 0 and 1, -1 to 5 voxel 0).
/// Returns the first voxel and how many on each axis (0 when inverted).
pub fn region_voxels(min: Vec3, max: Vec3) -> (IVec3, IVec3) {
    let lo = (min / VOXEL_STUDS).trunc().as_ivec3();
    let hi = (max / VOXEL_STUDS).trunc().as_ivec3();
    (lo, (hi - lo).max(IVec3::ZERO))
}

impl Grid {
    /// Write `voxels` (X fastest, then Y, then Z, as [`Grid::read_block`]
    /// reads them) into the block.
    pub fn write_block(&mut self, bounds: Bounds, voxels: &[Voxel]) {
        for (v, voxel) in bounds.voxels().zip(voxels) {
            self.set(v, *voxel);
        }
    }

    /// The block's voxels as a grid of their own, starting at voxel 0
    /// (`CopyRegion`).
    pub fn copy(&self, bounds: Bounds) -> Grid {
        let mut out = Grid::new();
        for v in bounds.voxels() {
            let voxel = self.voxel(v);
            if voxel != Voxel::AIR {
                out.set(v - bounds.min, voxel);
            }
        }
        out
    }

    /// Put `size` voxels of a copied grid at `corner` (`PasteRegion`);
    /// its air replaces what is there only with `paste_empty` (observed).
    pub fn paste(&mut self, from: &Grid, size: IVec3, corner: IVec3, paste_empty: bool) {
        let Some(source) = Bounds::of_block(IVec3::ZERO, size) else {
            return;
        };
        for v in source.voxels() {
            let voxel = from.voxel(v);
            if paste_empty || voxel.material != TerrainMaterial::Air {
                self.set(corner + v, voxel);
            }
        }
    }

    /// How many voxels aren't air (`CountCells`).
    pub fn count_cells(&self) -> u64 {
        self.counts().iter().sum()
    }
}
