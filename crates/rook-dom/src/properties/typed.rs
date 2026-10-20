//! Properties read as Rust types. `None` when the instance or property is
//! missing or holds another type; callers pick their own defaults.

use glam::Vec3;
use rook_types::{
    CFrame, Color3, ColorSequence, MaterialColors, NumberRange, NumberSequence, Value, Vector2,
};

use super::*;

impl Dom {
    /// A numeric property (`float`, `double`, `int`, `int64`) as an `f64`.
    pub fn number_property(&self, id: InstanceId, name: &str) -> Option<f64> {
        self.property_by_name(id, name)?.as_number()
    }

    /// A numeric property as an `f32`, the precision most consumers use.
    pub fn float_property(&self, id: InstanceId, name: &str) -> Option<f32> {
        self.number_property(id, name).map(|n| n as f32)
    }

    pub fn bool_property(&self, id: InstanceId, name: &str) -> Option<bool> {
        self.property_by_name(id, name)?.as_bool()
    }

    pub fn string_property(&self, id: InstanceId, name: &str) -> Option<String> {
        match self.property_by_name(id, name)? {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// An enum property's value.
    pub fn enum_property(&self, id: InstanceId, name: &str) -> Option<u32> {
        self.property_by_name(id, name)?.as_enum()
    }

    /// An enum property's item name (`"Sphere"` for `Enum.ParticleEmitterShape.Sphere`).
    pub fn enum_item_property(&self, id: InstanceId, name: &str) -> Option<&'static str> {
        let inst = self.instances.get(id)?;
        let prop = self.db.find_canonical_property(inst.class, name)?;
        let PropertyType::Enum(e) = prop.ty else {
            return None;
        };
        let value = self.property(id, prop.id)?.as_enum()?;
        self.db.enum_info(e).item_name(value)
    }

    pub fn vector3_property(&self, id: InstanceId, name: &str) -> Option<Vec3> {
        self.property_by_name(id, name)?.as_vector3()
    }

    pub fn vector2_property(&self, id: InstanceId, name: &str) -> Option<Vector2> {
        match self.property_by_name(id, name)? {
            Value::Vector2(v) => Some(v),
            _ => None,
        }
    }

    pub fn cframe_property(&self, id: InstanceId, name: &str) -> Option<CFrame> {
        self.property_by_name(id, name)?.as_cframe()
    }

    /// A `Color3` or `Color3uint8` property as a `Color3`.
    pub fn color3_property(&self, id: InstanceId, name: &str) -> Option<Color3> {
        self.property_by_name(id, name)?.as_color3()
    }

    pub fn number_range_property(&self, id: InstanceId, name: &str) -> Option<NumberRange> {
        match self.property_by_name(id, name)? {
            Value::NumberRange(r) => Some(r),
            _ => None,
        }
    }

    /// The instance a reference property (`PrimaryPart`, `CurrentCamera`,
    /// ...) points at, if it's set.
    pub fn ref_property(&self, id: InstanceId, name: &str) -> Option<InstanceId> {
        match self.property_by_name(id, name)? {
            Value::Ref(Some(r)) => Some(InstanceId::from_ref(r)),
            _ => None,
        }
    }

    /// A `CFrame?` property (`WorldPivotData`) when it holds one.
    pub fn optional_cframe_property(&self, id: InstanceId, name: &str) -> Option<CFrame> {
        match self.property_by_name(id, name)? {
            Value::OptionalCFrame(c) => c,
            Value::CFrame(c) => Some(c),
            _ => None,
        }
    }

    /// The URL a content property holds (`MeshId`, `SoundId`, ...): empty
    /// when it names none.
    pub fn content_property(&self, id: InstanceId, name: &str) -> Option<String> {
        self.property_by_name(id, name)?
            .content_url()
            .map(str::to_owned)
    }

    /// The bytes of a `BinaryString` or `SharedString` property.
    pub fn binary_property(&self, id: InstanceId, name: &str) -> Option<Vec<u8>> {
        match self.property_by_name(id, name)? {
            Value::BinaryString(b) => Some(b.into_vec()),
            Value::SharedString(s) => Some(s.data().to_vec()),
            _ => None,
        }
    }

    pub fn number_sequence_property(&self, id: InstanceId, name: &str) -> Option<NumberSequence> {
        match self.property_by_name(id, name)? {
            Value::NumberSequence(s) => Some(s),
            _ => None,
        }
    }

    pub fn color_sequence_property(&self, id: InstanceId, name: &str) -> Option<ColorSequence> {
        match self.property_by_name(id, name)? {
            Value::ColorSequence(s) => Some(s),
            _ => None,
        }
    }

    pub fn material_colors_property(&self, id: InstanceId, name: &str) -> Option<MaterialColors> {
        match self.property_by_name(id, name)? {
            Value::MaterialColors(c) => Some(c),
            _ => None,
        }
    }
}
