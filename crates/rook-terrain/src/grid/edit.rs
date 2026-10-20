//! Scripts' edits to the voxels: `Terrain`'s fills, region writes and
//! copies, with the occupancy rules measured in Studio 0.742
//! (`docs/behavior/terrain.md`, "The Terrain API").
//!
//! A fill gives each voxel near the shape an occupancy byte (255 full)
//! from where the voxel's centre lies, then merges it into the voxel: a
//! solid material takes the voxel when it fills at least as much as what
//! is there, water only fills what no solid does, and Air carves.

mod fills;
mod regions;
mod shape;

use glam::{IVec3, Vec3};

use crate::TerrainMaterial;
use crate::grid::{Grid, VOXEL_STUDS, Voxel, chunk_of};
pub use fills::merge;
pub use regions::{REGION_MAX_VOXELS, region_voxels};
pub use shape::Shape;

/// Voxels from `min` to `max`, both inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub min: IVec3,
    pub max: IVec3,
}

impl Bounds {
    /// The voxels from `min` for `size`, or `None` when that's empty.
    pub fn of_block(min: IVec3, size: IVec3) -> Option<Bounds> {
        (size.cmpgt(IVec3::ZERO).all()).then(|| Bounds {
            min,
            max: min + size - IVec3::ONE,
        })
    }

    /// The chunks holding a voxel within `reach` voxels of these.
    pub fn chunks(self, reach: i32) -> impl Iterator<Item = IVec3> {
        let (lo, _) = chunk_of(self.min - reach);
        let (hi, _) = chunk_of(self.max + reach);
        Bounds { min: lo, max: hi }.voxels()
    }

    fn voxels(self) -> impl Iterator<Item = IVec3> {
        let Bounds { min, max } = self;
        (min.z..=max.z).flat_map(move |z| {
            (min.y..=max.y).flat_map(move |y| (min.x..=max.x).map(move |x| IVec3::new(x, y, z)))
        })
    }
}

#[cfg(test)]
mod tests;
