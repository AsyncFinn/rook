//! Attributes and tags.

use super::*;

pub(super) fn get_attribute(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let value = with_world(lua, |w| w.dom.attribute(this, &name).cloned());
    match value {
        Some(v) => Outcome::ret(lua, to_lua(lua, &v, None)?),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}

/// Attribute names: up to 100 characters of letters, digits and
/// underscores, not starting with `RBX` (Confirmed: creator-docs
/// Instance:SetAttribute).
fn valid_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with("RBX")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn attribute_value(v: &LuaValue) -> ApiResult<Option<Value>> {
    use crate::ctx::convert::datatype_value;
    Ok(match v {
        LuaValue::Nil => None,
        LuaValue::Boolean(b) => Some(Value::Bool(*b)),
        LuaValue::Integer(_) | LuaValue::Number(_) => Some(Value::Float64(
            crate::ctx::args::number_of(v).unwrap_or(0.0),
        )),
        LuaValue::String(s) => Some(Value::String(s.to_string_lossy())),
        LuaValue::Vector(v) => Some(Value::Vector3(crate::ctx::convert::from_lua_vector(*v))),
        LuaValue::UserData(u) => {
            if let Ok(item) = u.borrow::<crate::types::enums::LEnumItem>() {
                return Ok(Some(Value::EnumItem {
                    enum_type: database().enum_info(item.enum_id).name.to_owned(),
                    value: item.value,
                }));
            }
            match datatype_value(u) {
                Some(
                    v @ (Value::UDim(_)
                    | Value::UDim2(_)
                    | Value::BrickColor(_)
                    | Value::Color3(_)
                    | Value::Vector2(_)
                    | Value::CFrame(_)
                    | Value::NumberSequence(_)
                    | Value::ColorSequence(_)
                    | Value::NumberRange(_)
                    | Value::Rect(_)
                    | Value::Font(_)),
                ) => Some(v),
                _ => {
                    return Err(ScriptError(format!(
                        "{} is not a supported attribute type",
                        crate::ctx::args::type_name(&LuaValue::UserData(u.clone()))
                    )));
                }
            }
        }
        other => {
            return Err(ScriptError(format!(
                "{} is not a supported attribute type",
                crate::ctx::args::type_name(other)
            )));
        }
    })
}

pub(super) fn set_attribute(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    if !valid_attribute_name(&name) {
        return Err(ScriptError(format!(
            "Attribute name \"{name}\" is not valid"
        )));
    }
    let value = attribute_value(a.get(1))?;
    mutate(lua, |w| w.dom.set_attribute(this, &name, value))?;
    Outcome::ret(lua, ())
}

pub(super) fn get_attributes(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let attrs: Vec<(String, Value)> = with_world(lua, |w| {
        w.dom
            .get(this)
            .map(|i| {
                i.attributes
                    .iter()
                    .map(|(k, v)| (k.to_owned(), v.clone()))
                    .collect()
            })
            .unwrap_or_default()
    });
    let t = lua.create_table()?;
    for (k, v) in attrs {
        t.raw_set(k, to_lua(lua, &v, None)?)?;
    }
    Outcome::ret(lua, t)
}

pub(super) fn attribute_changed_signal(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    Outcome::ret(
        lua,
        LSignal {
            instance: this,
            kind: SignalKind::Attribute(name),
        },
    )
}

pub(super) fn add_tag(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let tag = a.string(0)?;
    mutate(lua, |w| w.dom.add_tag(this, &tag))?;
    Outcome::ret(lua, ())
}

pub(super) fn remove_tag(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let tag = a.string(0)?;
    mutate(lua, |w| w.dom.remove_tag(this, &tag))?;
    Outcome::ret(lua, ())
}

pub(super) fn has_tag(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let tag = a.string(0)?;
    Outcome::ret(lua, with_world(lua, |w| w.dom.has_tag(this, &tag)))
}

pub(super) fn get_tags(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let tags = with_world(lua, |w| {
        w.dom.get(this).map(|i| i.tags.clone()).unwrap_or_default()
    });
    Outcome::ret(lua, lua.create_sequence_from(tags)?)
}
