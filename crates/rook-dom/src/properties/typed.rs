//! Properties read as Rust types. `None` when the instance or property is
//! missing or holds another type; callers pick their own defaults.

use glam::Vec3;
use rook_types::{
    Axes, BrickColor, CFrame, Color3, ColorSequence, Faces, Font, MaterialColors, NumberRange,
    NumberSequence, Rect, UDim, UDim2, Value, Vector2,
};

use super::*;

impl Dom {
    /// A numeric property (`float`, `double`, `int`, `int64`) as an `f64`.
    pub fn number_property(&self, id: InstanceId, name: &str) -> Option<f64> {
        self.read_by_name(id, name, Value::as_number)
    }

    pub fn float_property(&self, id: InstanceId, name: &str) -> Option<f32> {
        self.number_property(id, name).map(|n| n as f32)
    }

    pub fn bool_property(&self, id: InstanceId, name: &str) -> Option<bool> {
        self.read_by_name(id, name, Value::as_bool)
    }

    /// A string property without copying it when it's kept as set (or is
    /// the `Name`); a class default is copied out.
    pub fn str_property(&self, id: InstanceId, name: &str) -> Option<std::borrow::Cow<'_, str>> {
        let inst = self.slot(id)?;
        let prop = self.db.find_canonical_property(inst.class, name)?.id;
        self.str_of(id, prop)
    }

    /// [`Dom::str_property`] by the property's id.
    pub fn str_of(&self, id: InstanceId, prop: PropertyId) -> Option<std::borrow::Cow<'_, str>> {
        use std::borrow::Cow;
        let inst = self.slot(id)?;
        if prop == self.name_prop {
            return Some(Cow::Borrowed(&inst.name));
        }
        match inst.properties.get(prop) {
            Some(Value::String(s)) => Some(Cow::Borrowed(s)),
            Some(_) => None,
            None => self
                .with_default(inst.class, prop, |v| match v {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
                .map(Cow::Owned),
        }
    }

    pub fn string_property(&self, id: InstanceId, name: &str) -> Option<String> {
        self.read_by_name(id, name, |v| match v {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
    }

    pub fn enum_property(&self, id: InstanceId, name: &str) -> Option<u32> {
        self.read_by_name(id, name, Value::as_enum)
    }

    /// An enum property's item name (`"Sphere"` for `Enum.ParticleEmitterShape.Sphere`).
    pub fn enum_item_property(&self, id: InstanceId, name: &str) -> Option<&'static str> {
        let inst = self.slot(id)?;
        let prop = self.db.find_canonical_property(inst.class, name)?;
        self.enum_item(id, prop.id)
    }

    /// [`Dom::enum_item_property`] by the property's id.
    pub fn enum_item(&self, id: InstanceId, prop: PropertyId) -> Option<&'static str> {
        let PropertyType::Enum(e) = self.db.property(prop).ty else {
            return None;
        };
        let value = self.read_property(id, prop, Value::as_enum)?;
        self.db.enum_info(e).item_name(value)
    }

    pub fn vector3_property(&self, id: InstanceId, name: &str) -> Option<Vec3> {
        self.read_by_name(id, name, Value::as_vector3)
    }

    pub fn vector2_property(&self, id: InstanceId, name: &str) -> Option<Vector2> {
        self.read_by_name(id, name, |v| match v {
            Value::Vector2(v) => Some(*v),
            _ => None,
        })
    }

    pub fn udim_property(&self, id: InstanceId, name: &str) -> Option<UDim> {
        self.read_by_name(id, name, |v| match v {
            Value::UDim(v) => Some(*v),
            _ => None,
        })
    }

    pub fn udim2_property(&self, id: InstanceId, name: &str) -> Option<UDim2> {
        self.read_by_name(id, name, |v| match v {
            Value::UDim2(v) => Some(*v),
            _ => None,
        })
    }

    pub fn rect_property(&self, id: InstanceId, name: &str) -> Option<Rect> {
        self.read_by_name(id, name, |v| match v {
            Value::Rect(v) => Some(*v),
            _ => None,
        })
    }

    pub fn cframe_property(&self, id: InstanceId, name: &str) -> Option<CFrame> {
        self.read_by_name(id, name, Value::as_cframe)
    }

    /// A `Color3` or `Color3uint8` property as a `Color3`.
    pub fn color3_property(&self, id: InstanceId, name: &str) -> Option<Color3> {
        self.read_by_name(id, name, Value::as_color3)
    }

    pub fn brick_color_property(&self, id: InstanceId, name: &str) -> Option<BrickColor> {
        self.read_by_name(id, name, |v| match v {
            Value::BrickColor(b) => Some(*b),
            _ => None,
        })
    }

    pub fn font_property(&self, id: InstanceId, name: &str) -> Option<Font> {
        self.read_by_name(id, name, |v| match v {
            Value::Font(f) => Some(f.clone()),
            _ => None,
        })
    }

    pub fn faces_property(&self, id: InstanceId, name: &str) -> Option<Faces> {
        self.read_by_name(id, name, |v| match v {
            Value::Faces(v) => Some(*v),
            _ => None,
        })
    }

    pub fn axes_property(&self, id: InstanceId, name: &str) -> Option<Axes> {
        self.read_by_name(id, name, |v| match v {
            Value::Axes(v) => Some(*v),
            _ => None,
        })
    }

    pub fn number_range_property(&self, id: InstanceId, name: &str) -> Option<NumberRange> {
        self.read_by_name(id, name, |v| match v {
            Value::NumberRange(r) => Some(*r),
            _ => None,
        })
    }

    /// The instance a reference property (`PrimaryPart`, `CurrentCamera`,
    /// ...) points at, if it's set.
    pub fn ref_property(&self, id: InstanceId, name: &str) -> Option<InstanceId> {
        self.read_by_name(id, name, |v| match v {
            Value::Ref(Some(r)) => Some(InstanceId::from_ref(*r)),
            _ => None,
        })
    }

    /// A `CFrame?` property (`WorldPivotData`) when it holds one.
    pub fn optional_cframe_property(&self, id: InstanceId, name: &str) -> Option<CFrame> {
        self.read_by_name(id, name, |v| match v {
            Value::OptionalCFrame(c) => *c,
            Value::CFrame(c) => Some(*c),
            _ => None,
        })
    }

    /// The URL a content property holds (`MeshId`, `SoundId`, ...): empty
    /// when it names none.
    pub fn content_property(&self, id: InstanceId, name: &str) -> Option<String> {
        self.read_by_name(id, name, |v| v.content_url().map(str::to_owned))
    }

    pub fn binary_property(&self, id: InstanceId, name: &str) -> Option<Vec<u8>> {
        self.read_by_name(id, name, |v| match v {
            Value::BinaryString(b) => Some(AsRef::<[u8]>::as_ref(b).to_vec()),
            Value::SharedString(s) => Some(s.data().to_vec()),
            _ => None,
        })
    }

    pub fn number_sequence_property(&self, id: InstanceId, name: &str) -> Option<NumberSequence> {
        self.read_by_name(id, name, |v| match v {
            Value::NumberSequence(s) => Some(s.clone()),
            _ => None,
        })
    }

    pub fn color_sequence_property(&self, id: InstanceId, name: &str) -> Option<ColorSequence> {
        self.read_by_name(id, name, |v| match v {
            Value::ColorSequence(s) => Some(s.clone()),
            _ => None,
        })
    }

    pub fn material_colors_property(&self, id: InstanceId, name: &str) -> Option<MaterialColors> {
        self.read_by_name(id, name, |v| match v {
            Value::MaterialColors(c) => Some(c.clone()),
            _ => None,
        })
    }
}
