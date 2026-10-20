//! `Enum.Material`: the one list of materials, their values and names,
//! shared by physics (`rook-physics`'s density and friction table) and
//! drawing (how each material looks, [`looks`]).

mod looks;

pub use looks::{MaterialSet, TilePattern};

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

    /// Whether MaterialVariants can stand in for it: every material but
    /// Neon, Glass, ForceField, Air and Water, which have no override
    /// property on MaterialService (observed in Studio 0.742: the
    /// override methods fail with "Material 'Neon' is not supported.").
    pub fn supports_variants(self) -> bool {
        use Material::*;
        !matches!(self, Neon | Glass | ForceField | Air | Water)
    }

    /// The MaterialService property naming the variant that overrides it
    /// (`WoodName`), for materials that [support variants](Self::supports_variants).
    pub fn override_property(self) -> Option<String> {
        self.supports_variants()
            .then(|| format!("{}Name", self.name()))
    }
}

#[cfg(test)]
mod tests;
