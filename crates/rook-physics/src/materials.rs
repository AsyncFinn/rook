//! Physical properties of materials: density, friction, elasticity and
//! their weights, and how two touching parts combine them.
//!
//! The table was read from `PhysicalProperties.new(material)` in Studio
//! 0.742. Combining was measured there too: friction is the two parts'
//! frictions averaged with FrictionWeight as the weights, and elasticity
//! the same with ElasticityWeight.

use rook_dom::{Dom, InstanceId};
use rook_types::{Material, PhysicalProperties, Value};

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

/// Each material's properties.
#[rustfmt::skip]
const TABLE: &[(Material, Properties)] = &[
    (Material::Plastic, PLASTIC),
    (Material::SmoothPlastic, props(0.7, 0.2, 0.5, 1.0, 1.0, 0.2)),
    (Material::Neon, props(0.7, 0.3, 0.2, 1.0, 1.0, 0.3)),
    (Material::Wood, props(0.35, 0.48, 0.2, 1.0, 1.0, 0.25)),
    (Material::WoodPlanks, props(0.35, 0.48, 0.2, 1.0, 1.0, 0.25)),
    (Material::Marble, props(2.563, 0.2, 0.17, 1.0, 1.0, 0.01)),
    (Material::Basalt, props(2.691, 0.7, 0.15, 0.3, 1.0, 0.2)),
    (Material::Slate, props(2.691, 0.4, 0.2, 1.0, 1.0, 0.03)),
    (Material::CrackedLava, props(2.691, 0.65, 0.15, 1.0, 1.0, 0.35)),
    (Material::Concrete, props(2.403, 0.7, 0.2, 0.3, 1.0, 0.05)),
    (Material::Limestone, props(2.691, 0.5, 0.15, 1.0, 1.0, 0.15)),
    (Material::Granite, props(2.691, 0.4, 0.2, 1.0, 1.0, 0.2)),
    (Material::Pavement, props(2.691, 0.5, 0.17, 0.3, 1.0, 0.6)),
    (Material::Brick, props(1.922, 0.8, 0.15, 0.3, 1.0, 0.35)),
    (Material::Pebble, props(2.403, 0.4, 0.17, 1.0, 1.5, 0.4)),
    (Material::Cobblestone, props(2.691, 0.5, 0.17, 1.0, 1.0, 0.45)),
    (Material::Rock, props(2.691, 0.5, 0.17, 1.0, 1.0, 0.15)),
    (Material::Sandstone, props(2.691, 0.5, 0.15, 5.0, 1.0, 0.2)),
    (Material::CorrodedMetal, props(7.85, 0.7, 0.2, 1.0, 1.0, 0.15)),
    (Material::DiamondPlate, props(7.85, 0.35, 0.25, 1.0, 1.0, 0.05)),
    (Material::Foil, props(2.7, 0.4, 0.25, 1.0, 1.0, 0.1)),
    (Material::Metal, props(7.85, 0.4, 0.25, 1.0, 1.0, 0.1)),
    (Material::Grass, props(0.9, 0.4, 0.1, 1.0, 1.5, 0.7)),
    (Material::LeafyGrass, props(0.9, 0.4, 0.1, 2.0, 2.0, 0.75)),
    (Material::Sand, props(1.602, 0.5, 0.05, 5.0, 2.5, 0.55)),
    (Material::Fabric, props(0.7, 0.35, 0.05, 1.0, 1.0, 0.65)),
    (Material::Snow, props(0.9, 0.3, 0.03, 3.0, 4.0, 0.8)),
    (Material::Mud, props(0.9, 0.3, 0.07, 3.0, 4.0, 0.65)),
    (Material::Ground, props(0.9, 0.45, 0.1, 1.0, 1.0, 0.6)),
    (Material::Asphalt, props(2.36, 0.8, 0.2, 0.3, 1.0, 0.4)),
    (Material::Salt, props(2.165, 0.5, 0.05, 1.0, 1.0, 0.15)),
    (Material::Ice, props(0.919, 0.02, 0.15, 3.0, 1.0, 0.2)),
    (Material::Glacier, props(0.919, 0.05, 0.15, 2.0, 1.0, 0.7)),
    (Material::Glass, props(2.403, 0.25, 0.2, 1.0, 1.0, 0.1)),
    (Material::ForceField, props(2.403, 0.25, 0.2, 1.0, 1.0, 0.0)),
    (Material::Air, props(0.01, 0.01, 0.01, 1.0, 1.0, 0.01)),
    (Material::Water, props(1.0, 0.005, 0.01, 1.0, 1.0, 0.01)),
    (Material::Cardboard, props(0.7, 0.5, 0.05, 1.0, 2.0, 0.55)),
    (Material::Carpet, props(1.1, 0.4, 0.25, 1.0, 2.0, 0.65)),
    (Material::CeramicTiles, props(2.4, 0.51, 0.2, 1.0, 1.0, 0.04)),
    (Material::ClayRoofTiles, props(2.0, 0.51, 0.2, 1.0, 1.0, 0.3)),
    (Material::RoofShingles, props(2.36, 0.8, 0.2, 0.3, 1.0, 0.3)),
    (Material::Leather, props(0.86, 0.35, 0.25, 1.0, 1.0, 0.65)),
    (Material::Plaster, props(0.75, 0.6, 0.2, 0.3, 1.0, 0.3)),
    (Material::Rubber, props(1.3, 1.5, 0.95, 3.0, 2.0, 0.5)),
];

/// The default properties of the `Enum.Material` with this value; Plastic's
/// for values that aren't a material (Approximation).
pub fn of_material(value: u32) -> Properties {
    let material = Material::of_part(Some(value));
    TABLE
        .iter()
        .find(|(m, _)| *m == material)
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

/// The name of the MaterialVariant that stands in for `m` across the
/// place (MaterialService's `<Material>Name`, set by
/// `SetBaseMaterialOverride`): the material's own name until one is set
/// (observed in Studio 0.742: a new place answers "Plastic" for Plastic;
/// an override set to "" reads ""). `None` for materials without one.
pub fn base_material_override(dom: &Dom, service: InstanceId, m: Material) -> Option<String> {
    let property = m.override_property()?;
    Some(
        dom.property_by_name(service, &property)
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| m.name().to_owned()),
    )
}

/// The first MaterialVariant among MaterialService's descendants with this
/// name and base material (observed: one inside a Folder is found, one of
/// another base material isn't).
pub fn find_variant(dom: &Dom, service: InstanceId, m: Material, name: &str) -> Option<InstanceId> {
    dom.descendants(service).into_iter().find(|&v| {
        dom.class_name(v) == "MaterialVariant"
            && dom.name(v) == name
            && dom
                .property_by_name(v, "BaseMaterial")
                .and_then(|b| b.as_enum())
                == Some(m.value())
    })
}

/// The MaterialVariant a part draws with (`docs/behavior/materials.md`):
/// the one its `MaterialVariant` names, or else the one MaterialService's
/// override names for its material, if a variant of that name exists for
/// its material. A name that matches no variant draws the plain material,
/// and so does a variant of another base material (observed: a Plastic
/// part naming a Brick variant drew plain Plastic).
pub fn part_variant(dom: &Dom, part: InstanceId) -> Option<InstanceId> {
    let service = dom.find_service("MaterialService")?;
    let material = Material::of_part(
        dom.property_by_name(part, "Material")
            .and_then(|v| v.as_enum()),
    );
    let own = ["MaterialVariant", "MaterialVariantSerialized"]
        .iter()
        .find_map(|p| dom.property_by_name(part, p)?.as_str().map(str::to_owned))
        .filter(|n| !n.is_empty());
    let name = match own {
        Some(name) => name,
        None => base_material_override(dom, service, material)?,
    };
    if name.is_empty() || name == material.name() {
        // The material's own name is the built-in material (Inference: a
        // variant can't take a built-in name; "'%s' is a reserved
        // MaterialVariant name" in Studio's strings).
        return None;
    }
    find_variant(dom, service, material, &name)
}
