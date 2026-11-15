use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;
use rook_reflection::database;
use rook_types::Value;

use crate::api::methods::{RawMethod, mutate, ret_instances};
use crate::api::signal::LSignal;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::convert::to_lua;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::runtime::signal::SignalKind;

pub(super) fn get_attribute(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
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
    use crate::runtime::ctx::convert::datatype_value;
    Ok(match v {
        LuaValue::Nil => None,
        LuaValue::Boolean(b) => Some(Value::Bool(*b)),
        LuaValue::Integer(_) | LuaValue::Number(_) => Some(Value::Float64(
            crate::runtime::ctx::args::number_of(v).unwrap_or(0.0),
        )),
        LuaValue::String(s) => Some(Value::String(s.to_string_lossy())),
        LuaValue::Vector(v) => Some(Value::Vector3(
            crate::runtime::ctx::convert::from_lua_vector(*v),
        )),
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
                        crate::runtime::ctx::args::type_name(&LuaValue::UserData(u.clone()))
                    )));
                }
            }
        }
        other => {
            return Err(ScriptError(format!(
                "{} is not a supported attribute type",
                crate::runtime::ctx::args::type_name(other)
            )));
        }
    })
}

pub(super) fn set_attribute(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
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

pub(super) fn get_attributes(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
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

pub(super) fn attribute_changed_signal(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    Outcome::ret(
        lua,
        LSignal {
            instance: this,
            kind: SignalKind::Attribute(name),
        },
    )
}

pub(super) fn add_tag(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let tag = a.string(0)?;
    mutate(lua, |w| w.dom.add_tag(this, &tag))?;
    Outcome::ret(lua, ())
}

pub(super) fn remove_tag(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let tag = a.string(0)?;
    mutate(lua, |w| w.dom.remove_tag(this, &tag))?;
    Outcome::ret(lua, ())
}

pub(super) fn has_tag(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let tag = a.string(0)?;
    Outcome::ret(lua, with_world(lua, |w| w.dom.has_tag(this, &tag)))
}

pub(super) fn get_tags(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let tags = with_world(lua, |w| {
        w.dom
            .get(this)
            .map(|i| i.tags().to_vec())
            .unwrap_or_default()
    });
    Outcome::ret(lua, lua.create_sequence_from(tags)?)
}

/// What a CollectionService tag method gives for a nil instance, which
/// has no tags: AddTag and RemoveTag do nothing and HasTag is false, the
/// tag still required but not checked; GetTags is empty (measured).
#[derive(Clone, Copy)]
pub(super) enum NullTags {
    Nothing,
    False,
    Empty,
}

/// CollectionService's forms of the tag methods, which take the instance
/// first (creator-docs CollectionService).
pub(super) fn collection_tag(
    lua: &Lua,
    a: &Args,
    method: fn(&Lua, InstanceId, &Args) -> ApiResult<Outcome>,
    null: NullTags,
) -> ApiResult<Outcome> {
    let rest = a.shifted(1);
    let Some(instance) = a.object(0)? else {
        return match null {
            NullTags::Empty => Outcome::ret(lua, lua.create_table()?),
            _ if rest.len() == 0 => Err(rest.missing(0)),
            NullTags::False => Outcome::ret(lua, false),
            NullTags::Nothing => Outcome::ret(lua, ()),
        };
    };
    method(lua, instance, &rest)
}

/// Every instance in the DataModel with `tag`.
pub(super) fn get_tagged(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let tag = a.string(0)?;
    let found = with_world(lua, |w| w.dom.tagged(&tag));
    ret_instances(lua, &found)
}

/// `GetInstanceAddedSignal` (`added`) or `GetInstanceRemovedSignal`
/// (conformance case `signals/tag_signals`).
pub(super) fn tag_signal(lua: &Lua, a: &Args, added: bool) -> ApiResult<Outcome> {
    let tag = a.string(0)?;
    if tag.is_empty() {
        let method = if added { "Added" } else { "Removed" };
        return Err(ScriptError(format!(
            "Empty tag string provided to GetInstance{method}Signal"
        )));
    }
    let instance = with_world(lua, |w| w.dom.root());
    let kind = SignalKind::Tag { tag, added };
    Outcome::ret(lua, LSignal { instance, kind })
}

/// Every tag some instance in the DataModel has.
pub(super) fn get_all_tags(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let tags = with_world(lua, |w| w.dom.all_tags());
    Outcome::ret(lua, lua.create_sequence_from(tags)?)
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Instance", "GetAttributeChangedSignal") => attribute_changed_signal,
        ("Instance", "GetAttribute") => get_attribute,
        ("Instance", "SetAttribute") => set_attribute,
        ("Instance", "GetAttributes") => get_attributes,
        ("Instance", "AddTag") => add_tag,
        ("Instance", "RemoveTag") => remove_tag,
        ("Instance", "HasTag") => has_tag,
        ("Instance", "GetTags") => get_tags,
        ("CollectionService", "AddTag") => {
            |l, _, a| collection_tag(l, a, add_tag, NullTags::Nothing)
        }
        ("CollectionService", "RemoveTag") => {
            |l, _, a| collection_tag(l, a, remove_tag, NullTags::Nothing)
        }
        ("CollectionService", "HasTag") => |l, _, a| collection_tag(l, a, has_tag, NullTags::False),
        ("CollectionService", "GetTags") => {
            |l, _, a| collection_tag(l, a, get_tags, NullTags::Empty)
        }
        ("CollectionService", "GetTagged") => get_tagged,
        ("CollectionService", "GetAllTags") => get_all_tags,
        ("CollectionService", "GetInstanceAddedSignal") => |l, _, a| tag_signal(l, a, true),
        ("CollectionService", "GetInstanceRemovedSignal") => |l, _, a| tag_signal(l, a, false),
        _ => return None,
    })
}
