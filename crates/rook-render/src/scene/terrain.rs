use rook_terrain::Grid;
use rook_terrain::grid::edit::Bounds;

use super::*;

/// How the terrain's water and grass move (`docs/behavior/terrain/water.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainLook {
    /// `Terrain.WaterWaveSize`: how high the waves are, in studs.
    pub wave_size: f32,
    /// `Terrain.WaterWaveSpeed`.
    pub wave_speed: f32,
    /// `Workspace.GlobalWind`, studs per second: grass bends with it.
    pub wind: Vec3,
    /// How far from the camera grass blades are drawn; they shrink away
    /// over the last quarter.
    pub grass_reach: f32,
}

impl Default for TerrainLook {
    fn default() -> Self {
        Self {
            wave_size: 0.15,
            wave_speed: 10.0,
            wind: Vec3::ZERO,
            grass_reach: 0.0,
        }
    }
}

/// The terrain's voxels as the light voxels take them (they dim the sun
/// and the sky's light: docs/behavior/lighting/sun.md), sharing the
/// DataModel's chunks (copy on write).
#[derive(Debug, Clone, PartialEq)]
pub enum TerrainVoxels {
    /// All of them, in place of what the renderer had.
    Whole(Grid),
    /// The voxels in `bounds` changed: `chunks` holds the chunks holding
    /// them, as they are now (`Grid::region`).
    Edits { chunks: Grid, bounds: Vec<Bounds> },
}
