//! Property values ↔ Lua values.

use mlua::{Lua, Value as LuaValue};
use rbx_types::VariantType;
use rook_dom::InstanceId;
use rook_reflection::{EnumId, PropertyType};
use rook_types::{Color3, Value};

use crate::api::instance::{LuaInstance, push_instance};
use crate::ctx::args::{borrow_clone, number_of, type_name};
use crate::types::{
    LBrickColor, LCFrame, LColor3, LColorSequence, LNumberRange, LNumberSequence, LRay, LRect,
    LRegion3, LRegion3int16, LUDim, LUDim2, LVector2, LVector2int16, LVector3int16, Opaque, enums,
};

/// Luau's native `vector` → `Vector3`.
pub(crate) fn from_lua_vector(v: mlua::Vector) -> rook_types::Vector3 {
    rook_types::Vector3::new(v.x(), v.y(), v.z())
}

/// `Vector3` → Luau's native `vector`.
pub(crate) fn lua_vector(v: rook_types::Vector3) -> mlua::Vector {
    mlua::Vector::new(v.x(), v.y(), v.z())
}

pub(crate) fn vector3_to_lua(v: rook_types::Vector3) -> LuaValue {
    LuaValue::Vector(lua_vector(v))
}

/// Convert a stored value for scripts. `enum_id` is the property's enum,
/// for values stored as a bare `Value::Enum`.
pub(crate) fn to_lua(lua: &Lua, value: &Value, enum_id: Option<EnumId>) -> mlua::Result<LuaValue> {
    use mlua::IntoLua;
    if let Some(n) = value.as_number() {
        return Ok(LuaValue::Number(n));
    }
    Ok(match value {
        Value::Bool(b) => LuaValue::Boolean(*b),
        Value::String(s) => LuaValue::String(lua.create_string(s)?),
        Value::BinaryString(b) => LuaValue::String(lua.create_string(AsRef::<[u8]>::as_ref(b))?),
        Value::ContentId(c) => LuaValue::String(lua.create_string(c.as_str())?),
        Value::Vector3(v) => vector3_to_lua(*v),
        Value::Vector2(v) => LVector2(*v).into_lua(lua)?,
        Value::CFrame(c) => LCFrame(*c).into_lua(lua)?,
        Value::OptionalCFrame(Some(c)) => LCFrame(*c).into_lua(lua)?,
        Value::OptionalCFrame(None) => LuaValue::Nil,
        Value::Color3(c) => LColor3(*c).into_lua(lua)?,
        Value::Color3uint8(c) => LColor3(Color3::from(*c)).into_lua(lua)?,
        Value::BrickColor(b) => LBrickColor(*b).into_lua(lua)?,
        Value::UDim(u) => LUDim(*u).into_lua(lua)?,
        Value::UDim2(u) => LUDim2(*u).into_lua(lua)?,
        Value::NumberRange(r) => LNumberRange(*r).into_lua(lua)?,
        Value::NumberSequence(s) => LNumberSequence(s.clone()).into_lua(lua)?,
        Value::ColorSequence(s) => LColorSequence(s.clone()).into_lua(lua)?,
        Value::Ray(r) => LRay(*r).into_lua(lua)?,
        Value::Rect(r) => LRect(*r).into_lua(lua)?,
        Value::Region3(r) => LRegion3(*r).into_lua(lua)?,
        Value::Region3int16(r) => LRegion3int16(*r).into_lua(lua)?,
        Value::Vector3int16(v) => LVector3int16(*v).into_lua(lua)?,
        Value::Vector2int16(v) => LVector2int16(*v).into_lua(lua)?,
        Value::Enum(v) => match enum_id {
            Some(e) => enums::push_item(lua, e, *v)?,
            None => LuaValue::Number(f64::from(*v)),
        },
        Value::EnumItem { enum_type, value } => {
            match rook_reflection::database().enum_by_name(enum_type) {
                Some(e) => enums::push_item(lua, e.id, *value)?,
                None => LuaValue::Number(f64::from(*value)),
            }
        }
        Value::Ref(None) => LuaValue::Nil,
        Value::Ref(Some(r)) => LuaValue::UserData(push_instance(lua, InstanceId::from_ref(*r))?),
        other => Opaque(other.clone()).into_lua(lua)?,
    })
}

fn expected(ty: &str, got: &LuaValue) -> String {
    format!("{ty} expected, got {}", type_name(got))
}

/// Convert a script value for a property of type `ty`. The error is the
/// part after "Unable to assign property X. ".
pub(crate) fn from_lua(lua: &Lua, v: &LuaValue, ty: PropertyType) -> Result<Value, String> {
    let ud = |v: &LuaValue| match v {
        LuaValue::UserData(ud) => Some(ud.clone()),
        _ => None,
    };
    match ty {
        PropertyType::Enum(e) => enums::coerce(lua, v, e).map(Value::Enum),
        PropertyType::Value(vt) => {
            let fail = || expected(variant_name(vt), v);
            Ok(match vt {
                // Confirmed (conformance instance/property_coercion): booleans
                // take Lua truthiness, so "", 0 and {} are true, nil is false.
                VariantType::Bool => {
                    Value::Bool(!matches!(v, LuaValue::Nil | LuaValue::Boolean(false)))
                }
                VariantType::Float32 => Value::Float32(loose_number(v).ok_or_else(fail)? as f32),
                VariantType::Float64 => Value::Float64(loose_number(v).ok_or_else(fail)?),
                VariantType::Int32 => {
                    Value::Int32(loose_number(v).ok_or_else(fail)?.clamp(-2.1e9, 2.1e9) as i32)
                }
                VariantType::Int64 => Value::Int64(loose_number(v).ok_or_else(fail)? as i64),
                VariantType::String => Value::String(lua_string(v).ok_or_else(fail)?),
                VariantType::ContentId => Value::ContentId(lua_string(v).ok_or_else(fail)?.into()),
                VariantType::BinaryString => {
                    Value::BinaryString(lua_string(v).ok_or_else(fail)?.into_bytes().into())
                }
                VariantType::Vector3 => match v {
                    LuaValue::Vector(v) => Value::Vector3(from_lua_vector(*v)),
                    _ => return Err(fail()),
                },
                VariantType::Ref => match v {
                    LuaValue::Nil => Value::Ref(None),
                    LuaValue::UserData(u) => match u.borrow::<LuaInstance>() {
                        Ok(i) => Value::Ref(Some(i.0.to_ref())),
                        Err(_) => return Err(fail()),
                    },
                    _ => return Err(fail()),
                },
                VariantType::OptionalCFrame => match v {
                    LuaValue::Nil => Value::OptionalCFrame(None),
                    _ => Value::OptionalCFrame(Some(
                        ud(v)
                            .and_then(|u| borrow_clone::<LCFrame>(&u))
                            .ok_or_else(fail)?
                            .0,
                    )),
                },
                VariantType::Color3uint8 => Value::Color3uint8(
                    ud(v)
                        .and_then(|u| borrow_clone::<LColor3>(&u))
                        .ok_or_else(fail)?
                        .0
                        .into(),
                ),
                _ => {
                    let u = ud(v).ok_or_else(fail)?;
                    let value = datatype_value(&u).ok_or_else(fail)?;
                    if value_type(&value) != Some(vt) {
                        return Err(fail());
                    }
                    value
                }
            })
        }
    }
}

/// Numbers for number properties. Confirmed (conformance
/// instance/property_coercion): unparsable strings and booleans become 0.
/// Whether nil and other types also do is Inference; they raise for now.
fn loose_number(v: &LuaValue) -> Option<f64> {
    match v {
        LuaValue::String(_) => Some(number_of(v).unwrap_or(0.0)),
        LuaValue::Boolean(_) => Some(0.0),
        _ => number_of(v),
    }
}

fn lua_string(v: &LuaValue) -> Option<String> {
    match v {
        LuaValue::String(s) => Some(s.to_string_lossy()),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            number_of(v).map(crate::ctx::args::format_number)
        }
        _ => None,
    }
}

/// The stored value a datatype userdata represents.
pub(crate) fn datatype_value(u: &mlua::AnyUserData) -> Option<Value> {
    macro_rules! try_types {
        ($($t:ident => $variant:expr),* $(,)?) => {
            $(if let Some(x) = borrow_clone::<$t>(u) {
                #[allow(clippy::redundant_closure_call)]
                return Some(($variant)(x));
            })*
        };
    }
    try_types!(
        LVector2 => |x: LVector2| Value::Vector2(x.0),
        LCFrame => |x: LCFrame| Value::CFrame(x.0),
        LColor3 => |x: LColor3| Value::Color3(x.0),
        LBrickColor => |x: LBrickColor| Value::BrickColor(x.0),
        LUDim => |x: LUDim| Value::UDim(x.0),
        LUDim2 => |x: LUDim2| Value::UDim2(x.0),
        LNumberRange => |x: LNumberRange| Value::NumberRange(x.0),
        LNumberSequence => |x: LNumberSequence| Value::NumberSequence(x.0),
        LColorSequence => |x: LColorSequence| Value::ColorSequence(x.0),
        LRay => |x: LRay| Value::Ray(x.0),
        LRect => |x: LRect| Value::Rect(x.0),
        LRegion3 => |x: LRegion3| Value::Region3(x.0),
        LRegion3int16 => |x: LRegion3int16| Value::Region3int16(x.0),
        LVector3int16 => |x: LVector3int16| Value::Vector3int16(x.0),
        LVector2int16 => |x: LVector2int16| Value::Vector2int16(x.0),
        Opaque => |x: Opaque| x.0,
    );
    None
}

fn value_type(v: &Value) -> Option<VariantType> {
    Some(v.to_variant(|_| rbx_types::Ref::none()).ty())
}

/// Roblox's name for a value type in assignment errors.
pub(crate) fn variant_name(vt: VariantType) -> &'static str {
    match vt {
        VariantType::Bool => "bool",
        VariantType::Float32 => "float",
        VariantType::Float64 => "double",
        VariantType::Int32 => "int",
        VariantType::Int64 => "int64",
        VariantType::String | VariantType::BinaryString => "string",
        VariantType::ContentId => "ContentId",
        VariantType::Content => "Content",
        VariantType::Vector3 => "Vector3",
        VariantType::Vector2 => "Vector2",
        // Confirmed: Roblox still calls it CoordinateFrame in these messages.
        VariantType::CFrame | VariantType::OptionalCFrame => "CoordinateFrame",
        VariantType::Color3 | VariantType::Color3uint8 => "Color3",
        VariantType::BrickColor => "BrickColor",
        VariantType::UDim => "UDim",
        VariantType::UDim2 => "UDim2",
        VariantType::Ref => "Object",
        VariantType::NumberRange => "NumberRange",
        VariantType::NumberSequence => "NumberSequence",
        VariantType::ColorSequence => "ColorSequence",
        VariantType::Ray => "Ray",
        VariantType::Rect => "Rect",
        VariantType::Region3 => "Region3",
        VariantType::Region3int16 => "Region3int16",
        VariantType::Faces => "Faces",
        VariantType::Axes => "Axes",
        VariantType::Font => "Font",
        VariantType::PhysicalProperties => "PhysicalProperties",
        VariantType::Vector3int16 => "Vector3int16",
        VariantType::Vector2int16 => "Vector2int16",
        _ => "value",
    }
}
