//! `Enum.Material`: the one list of materials, their values and names,
//! shared by physics (`rook-physics`'s density and friction table) and
//! drawing (how each material shades).

/// `Enum.Material`, by value (read from Studio 0.742's
/// `Enum.Material:GetEnumItems()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Material {
    Plastic = 256,
    SmoothPlastic = 272,
    Neon = 288,
    Wood = 512,
    WoodPlanks = 528,
    Marble = 784,
    Basalt = 788,
    Slate = 800,
    CrackedLava = 804,
    Concrete = 816,
    Limestone = 820,
    Granite = 832,
    Pavement = 836,
    Brick = 848,
    Pebble = 864,
    Cobblestone = 880,
    Rock = 896,
    Sandstone = 912,
    CorrodedMetal = 1040,
    DiamondPlate = 1056,
    Foil = 1072,
    Metal = 1088,
    Grass = 1280,
    LeafyGrass = 1284,
    Sand = 1296,
    Fabric = 1312,
    Snow = 1328,
    Mud = 1344,
    Ground = 1360,
    Asphalt = 1376,
    Salt = 1392,
    Ice = 1536,
    Glacier = 1552,
    Glass = 1568,
    ForceField = 1584,
    Air = 1792,
    Water = 2048,
    Cardboard = 2304,
    Carpet = 2305,
    CeramicTiles = 2306,
    ClayRoofTiles = 2307,
    RoofShingles = 2308,
    Leather = 2309,
    Plaster = 2310,
    Rubber = 2311,
}

/// How a material shades without textures: its roughness and metalness,
/// 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shading {
    pub roughness: f32,
    pub metalness: f32,
}

impl Material {
    /// Every material, in `Enum.Material` order.
    pub const ALL: [Material; 45] = {
        use Material::*;
        [
            Plastic,
            SmoothPlastic,
            Neon,
            Wood,
            WoodPlanks,
            Marble,
            Basalt,
            Slate,
            CrackedLava,
            Concrete,
            Limestone,
            Granite,
            Pavement,
            Brick,
            Pebble,
            Cobblestone,
            Rock,
            Sandstone,
            CorrodedMetal,
            DiamondPlate,
            Foil,
            Metal,
            Grass,
            LeafyGrass,
            Sand,
            Fabric,
            Snow,
            Mud,
            Ground,
            Asphalt,
            Salt,
            Ice,
            Glacier,
            Glass,
            ForceField,
            Air,
            Water,
            Cardboard,
            Carpet,
            CeramicTiles,
            ClayRoofTiles,
            RoofShingles,
            Leather,
            Plaster,
            Rubber,
        ]
    };

    pub fn from_value(value: u32) -> Option<Self> {
        Self::ALL.iter().copied().find(|m| *m as u32 == value)
    }

    /// The material a part draws and simulates with: its `Material`, or
    /// Plastic when it has none saved (old files) or a value the enum
    /// doesn't have (Approximation, as physics has always done).
    pub fn of_part(value: Option<u32>) -> Self {
        value.and_then(Self::from_value).unwrap_or(Self::Plastic)
    }

    pub fn value(self) -> u32 {
        self as u32
    }

    /// The enum item's name, which is also the name MaterialService's
    /// override properties are built from (`WoodName`).
    pub fn name(self) -> &'static str {
        use Material::*;
        match self {
            Plastic => "Plastic",
            SmoothPlastic => "SmoothPlastic",
            Neon => "Neon",
            Wood => "Wood",
            WoodPlanks => "WoodPlanks",
            Marble => "Marble",
            Basalt => "Basalt",
            Slate => "Slate",
            CrackedLava => "CrackedLava",
            Concrete => "Concrete",
            Limestone => "Limestone",
            Granite => "Granite",
            Pavement => "Pavement",
            Brick => "Brick",
            Pebble => "Pebble",
            Cobblestone => "Cobblestone",
            Rock => "Rock",
            Sandstone => "Sandstone",
            CorrodedMetal => "CorrodedMetal",
            DiamondPlate => "DiamondPlate",
            Foil => "Foil",
            Metal => "Metal",
            Grass => "Grass",
            LeafyGrass => "LeafyGrass",
            Sand => "Sand",
            Fabric => "Fabric",
            Snow => "Snow",
            Mud => "Mud",
            Ground => "Ground",
            Asphalt => "Asphalt",
            Salt => "Salt",
            Ice => "Ice",
            Glacier => "Glacier",
            Glass => "Glass",
            ForceField => "ForceField",
            Air => "Air",
            Water => "Water",
            Cardboard => "Cardboard",
            Carpet => "Carpet",
            CeramicTiles => "CeramicTiles",
            ClayRoofTiles => "ClayRoofTiles",
            RoofShingles => "RoofShingles",
            Leather => "Leather",
            Plaster => "Plaster",
            Rubber => "Rubber",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|m| m.name() == name)
    }

    /// Only Plastic shows the surface patterns (observed in Studio 0.742:
    /// Studs and Inlet on every other material, SmoothPlastic included,
    /// drew nothing).
    pub fn shows_surfaces(self) -> bool {
        self == Self::Plastic
    }

    /// Roughness and metalness without the material's textures: the
    /// averages of the red (metalness) and green (roughness) channels of
    /// Studio 0.742's own roughness-metalness map for each material, as
    /// its files record them (`docs/behavior/materials.md`). Plastic,
    /// SmoothPlastic, Neon, ForceField, Air and Water have no such map;
    /// theirs are Approximations, tuned against Studio screenshots.
    pub fn shading(self) -> Shading {
        use Material::*;
        let (metalness, roughness) = match self {
            Plastic => (0.0, 0.75),
            SmoothPlastic => (0.0, 0.5),
            Neon | ForceField | Air | Water => (0.0, 1.0),
            Wood => (0.09, 0.53),
            WoodPlanks => (0.24, 0.65),
            Marble => (0.0, 0.49),
            Basalt => (0.0, 0.81),
            Slate => (0.0, 0.55),
            CrackedLava => (0.0, 0.74),
            Concrete => (0.0, 0.51),
            Limestone => (0.0, 0.67),
            Granite => (0.0, 0.58),
            Pavement => (0.0, 0.76),
            Brick => (0.0, 0.96),
            Pebble => (0.0, 0.6),
            Cobblestone => (0.0, 0.5),
            Rock => (0.2, 0.69),
            Sandstone => (0.0, 0.76),
            CorrodedMetal => (0.53, 0.73),
            DiamondPlate => (0.66, 0.42),
            Foil => (1.0, 0.17),
            Metal => (0.79, 0.35),
            Grass => (0.0, 0.83),
            LeafyGrass => (0.0, 0.91),
            Sand => (0.0, 0.53),
            Fabric => (0.0, 0.94),
            Snow => (0.0, 0.49),
            Mud => (0.0, 0.54),
            Ground => (0.0, 0.91),
            Asphalt => (0.0, 0.88),
            Salt => (0.0, 0.64),
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
    fn values_and_names_round_trip() {
        for m in Material::ALL {
            assert_eq!(Material::from_value(m.value()), Some(m));
            assert_eq!(Material::from_name(m.name()), Some(m));
        }
        assert_eq!(Material::of_part(None), Material::Plastic);
        assert_eq!(Material::of_part(Some(9999)), Material::Plastic);
        assert_eq!(Material::of_part(Some(288)), Material::Neon);
        assert!(Material::Plastic.shows_surfaces() && !Material::SmoothPlastic.shows_surfaces());
    }
}
