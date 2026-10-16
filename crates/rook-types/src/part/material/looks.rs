//! How each material looks: which surfaces it shows, how it shades, and
//! how large its textures are, in each of the two sets Studio draws
//! (`docs/behavior/materials.md`).

use super::Material;

/// Which textures the built-in materials draw with, picked per place by
/// `MaterialService.Use2022Materials` (observed in Studio 0.742: a place
/// saved with it true fetched and drew a different set from one saved
/// with it false, without it, or without a MaterialService at all).
/// Materials added in 2022 or later (Cardboard, Carpet, ...) have one set
/// of textures shared by both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MaterialSet {
    /// The materials before 2022: greyscale textures the part's Color
    /// tints. What places without `Use2022Materials` get.
    Legacy,
    /// The 2022 materials.
    Materials2022,
}

impl MaterialSet {
    /// The set of a place whose `Use2022Materials` is `on` (`None` when it
    /// isn't saved, or there is no MaterialService).
    pub fn of_place(use_2022_materials: Option<bool>) -> Self {
        match use_2022_materials {
            Some(true) => Self::Materials2022,
            _ => Self::Legacy,
        }
    }
}

/// How a material shades without textures: its roughness and metalness,
/// 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shading {
    pub roughness: f32,
    pub metalness: f32,
}

impl Material {
    /// Only Plastic shows the surface patterns (observed in Studio 0.742:
    /// Studs and Inlet on every other material, SmoothPlastic included,
    /// drew nothing).
    pub fn shows_surfaces(self) -> bool {
        self == Self::Plastic
    }

    /// Whether Studio draws the material with textures (every material
    /// but these; Plastic's own subtle texture isn't drawn: Approximation).
    pub fn has_textures(self) -> bool {
        use Material::*;
        !matches!(
            self,
            Plastic | SmoothPlastic | Neon | ForceField | Air | Water
        )
    }

    /// Studs per repeat of the material's textures, across and down each
    /// face, measured in Studio 0.742 (`docs/behavior/materials.md`) by
    /// matching 24-stud faces against the textures themselves. The 2022
    /// set repeats every 8 studs (Confirmed for most; the few weak matches
    /// take 8 too). The legacy set: Confirmed where Studio repeats the
    /// textures plainly; where it breaks the repeats up (its Organic
    /// pattern), fitted from close-ups by the size of their features
    /// (Approximation: Cobblestone, Salt, Grass, Foil, and Granite,
    /// CorrodedMetal, Slate and Basalt by eye); 8 studs where neither
    /// settled it (Approximation).
    pub fn studs_per_tile(self, set: MaterialSet) -> f32 {
        use Material::*;
        if set == MaterialSet::Materials2022 {
            return 8.0;
        }
        match self {
            Glass => 4.0,
            Wood | DiamondPlate | Metal => 5.0,
            Fabric => 20.0 / 3.0,
            Marble | Pebble | Sand | Ice | Grass => 10.0,
            Foil => 12.0,
            Cobblestone | Slate | Basalt => 16.0,
            Salt => 20.0,
            Granite | CorrodedMetal => 24.0,
            _ => 8.0,
        }
    }

    /// Roughness and metalness without the material's textures: the
    /// averages of the red (metalness) and green (roughness) channels of
    /// the set's roughness-metalness map, as Studio's files record them
    /// (`docs/behavior/materials.md`). Plastic, SmoothPlastic, Neon,
    /// ForceField, Air and Water have no such map; theirs are
    /// Approximations, tuned against Studio screenshots.
    pub fn shading(self, set: MaterialSet) -> Shading {
        use Material::*;
        let modern = set == MaterialSet::Materials2022;
        let (metalness, roughness) = match self {
            Plastic => (0.0, 0.75),
            SmoothPlastic => (0.0, 0.5),
            Neon | ForceField | Air | Water => (0.0, 1.0),
            Wood if modern => (0.0, 0.63),
            Wood => (0.09, 0.53),
            WoodPlanks if modern => (0.0, 0.55),
            WoodPlanks => (0.24, 0.65),
            Marble if modern => (0.0, 0.43),
            Marble => (0.0, 0.49),
            Basalt => (0.0, 0.81),
            Slate if modern => (0.0, 0.58),
            Slate => (0.0, 0.55),
            CrackedLava => (0.0, 0.74),
            Concrete if modern => (0.0, 0.71),
            Concrete => (0.0, 0.51),
            Limestone => (0.0, 0.67),
            Granite if modern => (0.0, 0.16),
            Granite => (0.0, 0.58),
            Pavement => (0.0, 0.76),
            Brick if modern => (0.0, 0.6),
            Brick => (0.0, 0.96),
            Pebble if modern => (0.0, 0.84),
            Pebble => (0.0, 0.6),
            Cobblestone if modern => (0.0, 0.86),
            Cobblestone => (0.0, 0.5),
            Rock => (0.2, 0.69),
            Sandstone => (0.0, 0.76),
            CorrodedMetal if modern => (0.16, 0.6),
            CorrodedMetal => (0.53, 0.73),
            DiamondPlate if modern => (1.0, 0.35),
            DiamondPlate => (0.66, 0.42),
            Foil if modern => (1.0, 0.24),
            Foil => (1.0, 0.17),
            Metal if modern => (1.0, 0.18),
            Metal => (0.79, 0.35),
            Grass if modern => (0.0, 0.9),
            Grass => (0.0, 0.83),
            LeafyGrass => (0.0, 0.91),
            Sand if modern => (0.0, 0.75),
            Sand => (0.0, 0.53),
            Fabric if modern => (0.0, 0.89),
            Fabric => (0.0, 0.94),
            Snow => (0.0, 0.49),
            Mud => (0.0, 0.54),
            Ground => (0.0, 0.91),
            Asphalt => (0.0, 0.88),
            Salt => (0.0, 0.64),
            Ice if modern => (0.0, 0.39),
            Ice => (0.0, 0.18),
            Glacier => (0.0, 0.39),
            Glass => (0.0, 0.11),
            Cardboard => (0.0, 0.67),
            Carpet => (0.0, 0.81),
            CeramicTiles => (0.0, 0.29),
            ClayRoofTiles => (0.0, 0.57),
            RoofShingles => (0.0, 0.78),
            Leather => (0.0, 0.46),
            Plaster => (0.0, 0.95),
            Rubber => (0.18, 0.58),
        };
        Shading {
            roughness,
            metalness,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_follow_use_2022_materials() {
        assert_eq!(
            MaterialSet::of_place(Some(true)),
            MaterialSet::Materials2022
        );
        assert_eq!(MaterialSet::of_place(Some(false)), MaterialSet::Legacy);
        assert_eq!(MaterialSet::of_place(None), MaterialSet::Legacy);
        // Glass repeats every 4 studs in the legacy set, 8 in the 2022 set.
        assert_eq!(Material::Glass.studs_per_tile(MaterialSet::Legacy), 4.0);
        assert_eq!(
            Material::Glass.studs_per_tile(MaterialSet::Materials2022),
            8.0
        );
        assert_ne!(
            Material::Metal.shading(MaterialSet::Legacy),
            Material::Metal.shading(MaterialSet::Materials2022)
        );
    }
}
