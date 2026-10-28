//! How terrain's water and grass move.

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
