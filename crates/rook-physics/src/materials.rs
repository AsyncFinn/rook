//! Physical properties of materials: density, friction, elasticity and
//! their weights, and how two touching parts combine them.
//!
//! The table was read from `PhysicalProperties.new(material)` in Studio
//! 0.742. Combining was measured there too: friction is the two parts'
//! frictions averaged with FrictionWeight as the weights, and elasticity
//! the same with ElasticityWeight.

use rook_dom::{Dom, InstanceId};
use rook_types::{PhysicalProperties, Value};

/// One set of physical properties (a `PhysicalProperties` value).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Properties {
    pub density: f32,
    pub friction: f32,
    pub elasticity: f32,
    pub friction_weight: f32,
    pub elasticity_weight: f32,
    pub acoustic_absorption: f32,
}

impl Properties {
    /// `PhysicalProperties.new(density, friction, elasticity, ...)` clamps
    /// each value (observed): density to 0.0001–100, friction to 0–2,
    /// elasticity and acoustic absorption to 0–1, weights to 0–100.
    pub fn clamped(self) -> Self {
        Self {
            density: self.density.clamp(0.0001, 100.0),
            friction: self.friction.clamp(0.0, 2.0),
            elasticity: self.elasticity.clamp(0.0, 1.0),
            friction_weight: self.friction_weight.clamp(0.0, 100.0),
            elasticity_weight: self.elasticity_weight.clamp(0.0, 100.0),
            acoustic_absorption: self.acoustic_absorption.clamp(0.0, 1.0),
        }
    }
}

const fn props(d: f32, f: f32, e: f32, fw: f32, ew: f32, a: f32) -> Properties {
    Properties {
        density: d,
        friction: f,
        elasticity: e,
        friction_weight: fw,
        elasticity_weight: ew,
        acoustic_absorption: a,
    }
}

/// Plastic, the default material.
pub const PLASTIC: Properties = props(0.7, 0.3, 0.5, 1.0, 1.0, 0.3);

/// `Enum.Material` values and their properties.
const TABLE: &[(u32, Properties)] = &[
    (256, PLASTIC),
    (272, props(0.7, 0.2, 0.5, 1.0, 1.0, 0.2)), // SmoothPlastic
    (288, props(0.7, 0.3, 0.2, 1.0, 1.0, 0.3)), // Neon
    (512, props(0.35, 0.48, 0.2, 1.0, 1.0, 0.25)), // Wood
    (528, props(0.35, 0.48, 0.2, 1.0, 1.0, 0.25)), // WoodPlanks
    (784, props(2.563, 0.2, 0.17, 1.0, 1.0, 0.01)), // Marble
    (788, props(2.691, 0.7, 0.15, 0.3, 1.0, 0.2)), // Basalt
    (800, props(2.691, 0.4, 0.2, 1.0, 1.0, 0.03)), // Slate
    (804, props(2.691, 0.65, 0.15, 1.0, 1.0, 0.35)), // CrackedLava
    (816, props(2.403, 0.7, 0.2, 0.3, 1.0, 0.05)), // Concrete
    (820, props(2.691, 0.5, 0.15, 1.0, 1.0, 0.15)), // Limestone
    (832, props(2.691, 0.4, 0.2, 1.0, 1.0, 0.2)), // Granite
    (836, props(2.691, 0.5, 0.17, 0.3, 1.0, 0.6)), // Pavement
    (848, props(1.922, 0.8, 0.15, 0.3, 1.0, 0.35)), // Brick
    (864, props(2.403, 0.4, 0.17, 1.0, 1.5, 0.4)), // Pebble
    (880, props(2.691, 0.5, 0.17, 1.0, 1.0, 0.45)), // Cobblestone
    (896, props(2.691, 0.5, 0.17, 1.0, 1.0, 0.15)), // Rock
    (912, props(2.691, 0.5, 0.15, 5.0, 1.0, 0.2)), // Sandstone
    (1040, props(7.85, 0.7, 0.2, 1.0, 1.0, 0.15)), // CorrodedMetal
    (1056, props(7.85, 0.35, 0.25, 1.0, 1.0, 0.05)), // DiamondPlate
    (1072, props(2.7, 0.4, 0.25, 1.0, 1.0, 0.1)), // Foil
    (1088, props(7.85, 0.4, 0.25, 1.0, 1.0, 0.1)), // Metal
    (1280, props(0.9, 0.4, 0.1, 1.0, 1.5, 0.7)), // Grass
    (1284, props(0.9, 0.4, 0.1, 2.0, 2.0, 0.75)), // LeafyGrass
    (1296, props(1.602, 0.5, 0.05, 5.0, 2.5, 0.55)), // Sand
    (1312, props(0.7, 0.35, 0.05, 1.0, 1.0, 0.65)), // Fabric
    (1328, props(0.9, 0.3, 0.03, 3.0, 4.0, 0.8)), // Snow
    (1344, props(0.9, 0.3, 0.07, 3.0, 4.0, 0.65)), // Mud
    (1360, props(0.9, 0.45, 0.1, 1.0, 1.0, 0.6)), // Ground
    (1376, props(2.36, 0.8, 0.2, 0.3, 1.0, 0.4)), // Asphalt
    (1392, props(2.165, 0.5, 0.05, 1.0, 1.0, 0.15)), // Salt
    (1536, props(0.919, 0.02, 0.15, 3.0, 1.0, 0.2)), // Ice
    (1552, props(0.919, 0.05, 0.15, 2.0, 1.0, 0.7)), // Glacier
    (1568, props(2.403, 0.25, 0.2, 1.0, 1.0, 0.1)), // Glass
    (1584, props(2.403, 0.25, 0.2, 1.0, 1.0, 0.0)), // ForceField
    (1792, props(0.01, 0.01, 0.01, 1.0, 1.0, 0.01)), // Air
    (2048, props(1.0, 0.005, 0.01, 1.0, 1.0, 0.01)), // Water
    (2304, props(0.7, 0.5, 0.05, 1.0, 2.0, 0.55)), // Cardboard
    (2305, props(1.1, 0.4, 0.25, 1.0, 2.0, 0.65)), // Carpet
    (2306, props(2.4, 0.51, 0.2, 1.0, 1.0, 0.04)), // CeramicTiles
    (2307, props(2.0, 0.51, 0.2, 1.0, 1.0, 0.3)), // ClayRoofTiles
    (2308, props(2.36, 0.8, 0.2, 0.3, 1.0, 0.3)), // RoofShingles
    (2309, props(0.86, 0.35, 0.25, 1.0, 1.0, 0.65)), // Leather
    (2310, props(0.75, 0.6, 0.2, 0.3, 1.0, 0.3)), // Plaster
    (2311, props(1.3, 1.5, 0.95, 3.0, 2.0, 0.5)), // Rubber
];

/// The default properties of the `Enum.Material` with this value; Plastic's
/// for values not in the table (Approximation).
pub fn of_material(value: u32) -> Properties {
    TABLE
        .iter()
        .find(|(v, _)| *v == value)
        .map_or(PLASTIC, |(_, p)| *p)
}

/// The properties a part simulates with: its `CustomPhysicalProperties`
/// when set, otherwise its material's.
pub fn of_part(dom: &Dom, part: InstanceId) -> Properties {
    if let Some(Value::PhysicalProperties(PhysicalProperties::Custom(c))) =
        dom.property_by_name(part, "CustomPhysicalProperties")
    {
        return props(
            c.density(),
            c.friction(),
            c.elasticity(),
            c.friction_weight(),
            c.elasticity_weight(),
            c.acoustic_absorption(),
        );
    }
    match dom.property_by_name(part, "Material") {
        Some(Value::Enum(n)) => of_material(n),
        _ => PLASTIC,
    }
}

/// Two values averaged by weight, as touching parts combine friction and
/// elasticity. Two zero weights give the plain average (Approximation).
pub fn mix(a: f32, weight_a: f32, b: f32, weight_b: f32) -> f32 {
    let total = weight_a + weight_b;
    if total <= 0.0 {
        return (a + b) * 0.5;
    }
    (a * weight_a + b * weight_b) / total
}
