//! How terrain draws: its materials' layers and water.

use super::*;

/// How the terrain's water and grass move (`docs/behavior/terrain.md`).
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
    /// How each texture layer blends where layers meet on a seam, by the
    /// layer's index (`rook_terrain::Layer::index`).
    pub layers: [TerrainLayer; TERRAIN_LAYERS],
}

/// The most terrain texture layers: three facings of 23 materials.
pub const TERRAIN_LAYERS: usize = 69;

/// How a terrain texture layer weighs in on a seam: a terrain mesh whose
/// vertices name their triangle's corners' layers (`Mesh::terrain_corners`)
/// draws its share of the corners' blend, Studio's: each corner's
/// barycentric weight times its layer's height there, the highest winning
/// over a soft edge (read from Studio 0.742's terrain shader;
/// `docs/behavior/terrain.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainLayer {
    /// The image holding the layer's height map, in blue (its
    /// roughness-metalness map); none counts as full height.
    pub height: Option<ImageHash>,
    /// The height's multiplier and offset (`height * a + b`, 0 to 1).
    pub remap: [f32; 2],
    /// How soft the edge between layers is.
    pub scale: f32,
    /// Studs per repeat of its textures.
    pub studs: f32,
    /// Textures laid like a box's (Brick, WoodPlanks).
    pub cube: bool,
    /// How its height map is turned and shifted, as its textures are.
    pub detile: Detile,
}

impl Default for TerrainLayer {
    fn default() -> Self {
        Self {
            height: None,
            remap: [0.0, 1.0],
            scale: 1.0,
            studs: 8.0,
            cube: false,
            detile: Detile::NONE,
        }
    }
}

impl Default for TerrainLook {
    fn default() -> Self {
        Self {
            wave_size: 0.15,
            wave_speed: 10.0,
            wind: Vec3::ZERO,
            grass_reach: 0.0,
            layers: [TerrainLayer::default(); TERRAIN_LAYERS],
        }
    }
}
