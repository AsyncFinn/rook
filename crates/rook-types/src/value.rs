use rbx_types::Variant;

use crate::{CFrame, Color3, UDim, UDim2, Vector2, Vector3};

/// An opaque reference to an instance, as stored in a property.
///
/// Its meaning belongs to whichever DOM produced it; conversions to and from
/// `rbx_types::Ref` take a mapping function so each DOM supplies its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceRef(pub u64);

/// A dynamic property value.
///
/// There is exactly one variant per `rbx_types::Variant` variant, so
/// converting to `Value` and back never changes the data. Math types use
/// Rook's own representations; the rest reuse `rbx_types` structs.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Axes(rbx_types::Axes),
    BinaryString(rbx_types::BinaryString),
    Bool(bool),
    BrickColor(rbx_types::BrickColor),
    CFrame(CFrame),
    Color3(Color3),
    Color3uint8(rbx_types::Color3uint8),
    ColorSequence(rbx_types::ColorSequence),
    Content(rbx_types::Content),
    ContentId(rbx_types::ContentId),
    /// An enum value whose enum type is known from the property's descriptor.
    Enum(u32),
    /// An enum value that carries its enum type name (used by attributes).
    EnumItem {
        enum_type: String,
        value: u32,
    },
    Faces(rbx_types::Faces),
    Float32(f32),
    Float64(f64),
    Font(rbx_types::Font),
    Int32(i32),
    Int64(i64),
    MaterialColors(rbx_types::MaterialColors),
    NetAssetRef(rbx_types::NetAssetRef),
    NumberRange(rbx_types::NumberRange),
    NumberSequence(rbx_types::NumberSequence),
    OptionalCFrame(Option<CFrame>),
    PhysicalProperties(rbx_types::PhysicalProperties),
    Ray(rbx_types::Ray),
    Rect(rbx_types::Rect),
    Ref(Option<InstanceRef>),
    Region3(rbx_types::Region3),
    Region3int16(rbx_types::Region3int16),
    SecurityCapabilities(rbx_types::SecurityCapabilities),
    SharedString(rbx_types::SharedString),
    String(String),
    Tags(rbx_types::Tags),
    UDim(UDim),
    UDim2(UDim2),
    UniqueId(rbx_types::UniqueId),
    Vector2(Vector2),
    Vector2int16(rbx_types::Vector2int16),
    Vector3(Vector3),
    Vector3int16(rbx_types::Vector3int16),
    Attributes(rbx_types::Attributes),
}

#[derive(Debug, thiserror::Error)]
pub enum ValueError {
    /// `rbx_types` added a value type Rook does not know about yet.
    #[error("unsupported value type {0:?}; rook-types needs updating")]
    UnsupportedType(rbx_types::VariantType),
}

impl Value {
    /// Convert from an `rbx_types::Variant`. `map_ref` translates referents
    /// into the caller's DOM; return `None` for a null or unknown referent.
    pub fn from_variant(
        variant: Variant,
        mut map_ref: impl FnMut(rbx_types::Ref) -> Option<InstanceRef>,
    ) -> Result<Self, ValueError> {
        Ok(match variant {
            Variant::Axes(v) => Self::Axes(v),
            Variant::BinaryString(v) => Self::BinaryString(v),
            Variant::Bool(v) => Self::Bool(v),
            Variant::BrickColor(v) => Self::BrickColor(v),
            Variant::CFrame(v) => Self::CFrame(v.into()),
            Variant::Color3(v) => Self::Color3(v.into()),
            Variant::Color3uint8(v) => Self::Color3uint8(v),
            Variant::ColorSequence(v) => Self::ColorSequence(v),
            Variant::Content(v) => Self::Content(v),
            Variant::ContentId(v) => Self::ContentId(v),
            Variant::Enum(v) => Self::Enum(v.to_u32()),
            Variant::EnumItem(v) => Self::EnumItem {
                enum_type: v.ty,
                value: v.value,
            },
            Variant::Faces(v) => Self::Faces(v),
            Variant::Float32(v) => Self::Float32(v),
            Variant::Float64(v) => Self::Float64(v),
            Variant::Font(v) => Self::Font(v),
            Variant::Int32(v) => Self::Int32(v),
            Variant::Int64(v) => Self::Int64(v),
            Variant::MaterialColors(v) => Self::MaterialColors(v),
            Variant::NetAssetRef(v) => Self::NetAssetRef(v),
            Variant::NumberRange(v) => Self::NumberRange(v),
            Variant::NumberSequence(v) => Self::NumberSequence(v),
            Variant::OptionalCFrame(v) => Self::OptionalCFrame(v.map(Into::into)),
            Variant::PhysicalProperties(v) => Self::PhysicalProperties(v),
            Variant::Ray(v) => Self::Ray(v),
            Variant::Rect(v) => Self::Rect(v),
            Variant::Ref(r) => Self::Ref(if r.is_none() { None } else { map_ref(r) }),
            Variant::Region3(v) => Self::Region3(v),
            Variant::Region3int16(v) => Self::Region3int16(v),
            Variant::SecurityCapabilities(v) => Self::SecurityCapabilities(v),
            Variant::SharedString(v) => Self::SharedString(v),
            Variant::String(v) => Self::String(v),
            Variant::Tags(v) => Self::Tags(v),
            Variant::UDim(v) => Self::UDim(v.into()),
            Variant::UDim2(v) => Self::UDim2(v.into()),
            Variant::UniqueId(v) => Self::UniqueId(v),
            Variant::Vector2(v) => Self::Vector2(v.into()),
            Variant::Vector2int16(v) => Self::Vector2int16(v),
            Variant::Vector3(v) => Self::Vector3(v.into()),
            Variant::Vector3int16(v) => Self::Vector3int16(v),
            Variant::Attributes(v) => Self::Attributes(v),
            other => return Err(ValueError::UnsupportedType(other.ty())),
        })
    }

    /// Convert to an `rbx_types::Variant`. `map_ref` translates the caller's
    /// references back into referents.
    pub fn to_variant(&self, mut map_ref: impl FnMut(InstanceRef) -> rbx_types::Ref) -> Variant {
        match self {
            Self::Axes(v) => Variant::Axes(*v),
            Self::BinaryString(v) => Variant::BinaryString(v.clone()),
            Self::Bool(v) => Variant::Bool(*v),
            Self::BrickColor(v) => Variant::BrickColor(*v),
            Self::CFrame(v) => Variant::CFrame((*v).into()),
            Self::Color3(v) => Variant::Color3((*v).into()),
            Self::Color3uint8(v) => Variant::Color3uint8(*v),
            Self::ColorSequence(v) => Variant::ColorSequence(v.clone()),
            Self::Content(v) => Variant::Content(v.clone()),
            Self::ContentId(v) => Variant::ContentId(v.clone()),
            Self::Enum(v) => Variant::Enum(rbx_types::Enum::from_u32(*v)),
            Self::EnumItem { enum_type, value } => Variant::EnumItem(rbx_types::EnumItem {
                ty: enum_type.clone(),
                value: *value,
            }),
            Self::Faces(v) => Variant::Faces(*v),
            Self::Float32(v) => Variant::Float32(*v),
            Self::Float64(v) => Variant::Float64(*v),
            Self::Font(v) => Variant::Font(v.clone()),
            Self::Int32(v) => Variant::Int32(*v),
            Self::Int64(v) => Variant::Int64(*v),
            Self::MaterialColors(v) => Variant::MaterialColors(v.clone()),
            Self::NetAssetRef(v) => Variant::NetAssetRef(v.clone()),
            Self::NumberRange(v) => Variant::NumberRange(*v),
            Self::NumberSequence(v) => Variant::NumberSequence(v.clone()),
            Self::OptionalCFrame(v) => Variant::OptionalCFrame(v.map(Into::into)),
            Self::PhysicalProperties(v) => Variant::PhysicalProperties(*v),
            Self::Ray(v) => Variant::Ray(*v),
            Self::Rect(v) => Variant::Rect(*v),
            Self::Ref(r) => Variant::Ref(r.map_or_else(rbx_types::Ref::none, &mut map_ref)),
            Self::Region3(v) => Variant::Region3(*v),
            Self::Region3int16(v) => Variant::Region3int16(*v),
            Self::SecurityCapabilities(v) => Variant::SecurityCapabilities(*v),
            Self::SharedString(v) => Variant::SharedString(v.clone()),
            Self::String(v) => Variant::String(v.clone()),
            Self::Tags(v) => Variant::Tags(v.clone()),
            Self::UDim(v) => Variant::UDim((*v).into()),
            Self::UDim2(v) => Variant::UDim2((*v).into()),
            Self::UniqueId(v) => Variant::UniqueId(*v),
            Self::Vector2(v) => Variant::Vector2((*v).into()),
            Self::Vector2int16(v) => Variant::Vector2int16(*v),
            Self::Vector3(v) => Variant::Vector3((*v).into()),
            Self::Vector3int16(v) => Variant::Vector3int16(*v),
            Self::Attributes(v) => Variant::Attributes(v.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(variant: Variant) {
        let value = Value::from_variant(variant.clone(), |_| Some(InstanceRef(7))).unwrap();
        let back = value.to_variant(|_| rbx_types::Ref::none());
        assert_eq!(back, variant);
    }

    #[test]
    fn lossless_round_trips() {
        round_trip(Variant::Bool(true));
        round_trip(Variant::Int32(-5));
        round_trip(Variant::Int64(1 << 40));
        round_trip(Variant::Float32(0.1));
        round_trip(Variant::Float64(0.1));
        round_trip(Variant::String("hello".into()));
        round_trip(Variant::Enum(rbx_types::Enum::from_u32(3)));
        round_trip(Variant::Vector3(rbx_types::Vector3::new(1.0, -2.0, 3.5)));
        round_trip(Variant::CFrame(rbx_types::CFrame::new(
            rbx_types::Vector3::new(1.0, 2.0, 3.0),
            rbx_types::Matrix3::new(
                rbx_types::Vector3::new(0.0, -1.0, 0.0),
                rbx_types::Vector3::new(1.0, 0.0, 0.0),
                rbx_types::Vector3::new(0.0, 0.0, 1.0),
            ),
        )));
        round_trip(Variant::OptionalCFrame(None));
        round_trip(Variant::UDim2(rbx_types::UDim2::new(
            rbx_types::UDim::new(0.5, 10),
            rbx_types::UDim::new(1.0, -3),
        )));
        round_trip(Variant::Color3(rbx_types::Color3::new(0.2, 0.4, 0.6)));
        round_trip(Variant::Color3uint8(rbx_types::Color3uint8::new(1, 2, 3)));
        round_trip(Variant::Ref(rbx_types::Ref::none()));
    }

    #[test]
    fn refs_are_mapped() {
        let value = Value::from_variant(Variant::Ref(rbx_types::Ref::new()), |_| {
            Some(InstanceRef(42))
        })
        .unwrap();
        assert_eq!(value, Value::Ref(Some(InstanceRef(42))));
    }
}
