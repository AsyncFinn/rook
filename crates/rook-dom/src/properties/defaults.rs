//! A property's default: Rook's own list, the database's class default,
//! the value measured in Studio, or the zero value of its type.

use super::*;

impl Dom {
    pub(super) fn default_value(&self, class: ClassId, property: PropertyId) -> Option<Value> {
        if let Some(v) = self.defaults.borrow().get(&(class, property)) {
            return Some(v.clone());
        }
        let v = convert_default(self.db, class, property)?;
        self.defaults
            .borrow_mut()
            .insert((class, property), v.clone());
        Some(v)
    }

    /// Whether `value` is the class default, without copying the default.
    pub(super) fn default_is(&self, class: ClassId, property: PropertyId, value: &Value) -> bool {
        if let Some(v) = self.defaults.borrow().get(&(class, property)) {
            return v == value;
        }
        self.default_value(class, property).as_ref() == Some(value)
    }
}

/// Defaults the database lists wrongly. Keyed by the class that declares
/// the property. Ones it leaves out come from the Studio sweep
/// ([`studio_defaults`]).
const DEFAULTS: &[(&str, &str, Value)] = &[
    // Confirmed: creator-docs, and Studio 0.742 (the database says false).
    ("SoundService", "RespectFilteringEnabled", Value::Bool(true)),
];

/// [`DEFAULTS`], then the class default from the database, then the value
/// measured in Studio (the database lists defaults only for properties
/// that serialize), then the zero value of the property's type.
fn convert_default(db: &Database, class: ClassId, property: PropertyId) -> Option<Value> {
    let prop = db.property(property);
    let ty = prop.ty;
    let owner = db.class(prop.owner).name;
    if let Some((_, _, v)) = DEFAULTS
        .iter()
        .find(|(c, p, _)| *c == owner && *p == prop.name)
    {
        return Some(v.clone());
    }
    if let Some(variant) = db.default_value(class, property) {
        return Value::from_variant(variant.clone(), |_| None)
            .ok()
            .map(|v| coerce(ty, v));
    }
    if let Some(v) = studio_defaults::measured(db.class(class).name, prop.name)
        .or_else(|| studio_defaults::measured(owner, prop.name))
    {
        return Some(v);
    }
    zero_value(db, ty)
}

/// What a property reads when nothing gives it a default: false, 0, empty,
/// or the enum's first item. `rook-tools studio-defaults` keeps only the
/// Studio values that differ from it.
pub fn zero_value(db: &Database, ty: PropertyType) -> Option<Value> {
    use rbx_types::VariantType as T;
    use rook_types::{CFrame, Color3, UDim, UDim2, Vector2, Vector3};
    Some(match ty {
        PropertyType::Enum(e) => Value::Enum(db.enum_info(e).items.first().map_or(0, |(_, v)| *v)),
        PropertyType::Value(t) => match t {
            T::Bool => Value::Bool(false),
            T::Float32 => Value::Float32(0.0),
            T::Float64 => Value::Float64(0.0),
            T::Int32 => Value::Int32(0),
            T::Int64 => Value::Int64(0),
            T::String => Value::String(String::new()),
            T::ContentId => Value::ContentId(String::new().into()),
            T::Vector3 => Value::Vector3(Vector3::ZERO),
            T::Vector2 => Value::Vector2(Vector2::ZERO),
            T::CFrame => Value::CFrame(CFrame::IDENTITY),
            T::Color3 => Value::Color3(Color3::new(0.0, 0.0, 0.0)),
            T::UDim => Value::UDim(UDim::default()),
            T::UDim2 => Value::UDim2(UDim2::default()),
            T::Ref => Value::Ref(None),
            T::BrickColor => Value::BrickColor(rbx_types::BrickColor::from_number(194)?),
            T::NumberRange => Value::NumberRange(rbx_types::NumberRange::new(0.0, 0.0)),
            _ => return None,
        },
    })
}
