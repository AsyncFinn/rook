//! BindableEvent and BindableFunction, and the callback members (`OnInvoke`,
//! `OnServerInvoke`, ...) scripts set. What a bindable's arguments and
//! results arrive as: tables are copied, once per listener, by rules of
//! their own (measured in Studio 0.746: `docs/behavior/scripting/bindables.md`).

use std::ffi::c_void;

use mlua::Table;

use mlua::{Function, Lua, MultiValue, Value as LuaValue};
use rook_dom::InstanceId;

use crate::api::methods::RawMethod;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::args::{display_value, type_name};
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError};
use crate::runtime::signal::SignalKey;
use crate::runtime::threads::fire_with;

/// Each listener gets its own copy, made now, not when it runs.
pub(super) fn bindable_fire(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let args = copy_values(lua, &a.rest(0))?;
    fire_with(lua, &SignalKey::event(this, "Event"), || {
        copy_values(lua, &args).unwrap_or_default()
    });
    Outcome::ret(lua, ())
}

/// `std.copy_bindable(...)`: what `Invoke`'s arguments and results become.
pub(crate) fn install(lua: &Lua, std: &mlua::Table) -> mlua::Result<()> {
    std.set(
        "copy_bindable",
        crate::runtime::ctx::api_fn(lua, |lua, args| {
            let copied = copy_values(lua, &args.into_vec())?;
            Ok(Outcome::Return(MultiValue::from_vec(copied)))
        })?,
    )
}

fn callbacks_table(lua: &Lua) -> mlua::Result<mlua::Table> {
    match lua.named_registry_value::<Option<mlua::Table>>("rook.callbacks")? {
        Some(t) => Ok(t),
        None => {
            // By instance key: a callback belongs to the instance, which
            // outlives the userdata it was set through.
            let t = lua.create_table()?;
            lua.set_named_registry_value("rook.callbacks", t.clone())?;
            Ok(t)
        }
    }
}

/// Drop `id`'s callbacks, as it's being freed.
pub(crate) fn forget_callbacks(lua: &Lua, id: InstanceId) -> mlua::Result<()> {
    match lua.named_registry_value::<Option<mlua::Table>>("rook.callbacks")? {
        Some(all) => all.raw_set(crate::api::instance::key_of(id), mlua::Value::Nil),
        None => Ok(()),
    }
}

/// Store a callback member (`OnInvoke`, `OnServerInvoke`, ...).
pub(crate) fn set_callback(
    lua: &Lua,
    id: InstanceId,
    name: &str,
    f: Option<Function>,
) -> mlua::Result<()> {
    let key = crate::api::instance::key_of(id);
    let all = callbacks_table(lua)?;
    let t: mlua::Table = match all.raw_get::<Option<mlua::Table>>(key)? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            all.raw_set(key, t.clone())?;
            t
        }
    };
    t.raw_set(name, f)
}

pub(crate) fn get_callback(
    lua: &Lua,
    id: InstanceId,
    name: &str,
) -> mlua::Result<Option<Function>> {
    let key = crate::api::instance::key_of(id);
    match callbacks_table(lua)?.raw_get::<Option<mlua::Table>>(key)? {
        Some(t) => t.raw_get(name),
        None => Ok(None),
    }
}

/// `Invoke`, written in Luau: it waits until a callback is set.
pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        ("BindableFunction", "Invoke") => {
            r#"
            local std = ...
            local wait_for_callback, copy = std.wait_for_callback, std.copy_bindable
            local function call(self, ...)
                return wait_for_callback(self, "OnInvoke")(...)
            end
            return function(self, ...)
                return copy(call(self, copy(...)))
            end
            "#
        }
        _ => return None,
    })
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("BindableEvent", "Fire") => bindable_fire,
        _ => return None,
    })
}

/// Copies of `values`. Fails on what a table can't carry.
fn copy_values(lua: &Lua, values: &[LuaValue]) -> Result<Vec<LuaValue>, ScriptError> {
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        out.push(copy_value(lua, v, &mut Vec::new())?);
    }
    Ok(out)
}

/// `within` holds the tables being copied: a table reached twice otherwise
/// becomes two copies, one reached from itself is an error.
fn copy_value(
    lua: &Lua,
    value: &LuaValue,
    within: &mut Vec<*const c_void>,
) -> Result<LuaValue, ScriptError> {
    Ok(match value {
        LuaValue::Table(t) => LuaValue::Table(copy_table(lua, t, within)?),
        LuaValue::Thread(_) => LuaValue::Nil,
        other => other.clone(),
    })
}

fn copy_table(
    lua: &Lua,
    table: &Table,
    within: &mut Vec<*const c_void>,
) -> Result<Table, ScriptError> {
    let cast = |e: mlua::Error| ScriptError(crate::runtime::ctx::lua_error_message(&e));
    let address = table.to_pointer();
    if within.contains(&address) {
        return Err(ScriptError("tables cannot be cyclic".into()));
    }
    within.push(address);
    let length = table.raw_len();
    let copy = lua.create_table_with_capacity(length, 0).map_err(cast)?;
    if length > 0 {
        // An array: holes inside it stay, its other keys don't.
        for i in 1..=length {
            let v: LuaValue = table.raw_get(i).map_err(cast)?;
            let v = copy_value(lua, &v, within)?;
            copy.raw_set(i, v).map_err(cast)?;
        }
    } else {
        for pair in table.clone().pairs::<LuaValue, LuaValue>() {
            let (k, v) = pair.map_err(cast)?;
            let key = dictionary_key(lua, &k)?;
            let v = copy_value(lua, &v, within)?;
            copy.raw_set(key, v).map_err(cast)?;
        }
    }
    within.pop();
    Ok(copy)
}

/// A dictionary's keys all arrive as strings.
fn dictionary_key(lua: &Lua, key: &LuaValue) -> Result<LuaValue, ScriptError> {
    let text = match key {
        LuaValue::String(_) => return Ok(key.clone()),
        LuaValue::Boolean(_) => return Err(ScriptError("Invalid table key type used".into())),
        LuaValue::Integer(i) => i.to_string(),
        LuaValue::Number(n) => whole(*n).to_string(),
        other => format!("<{}> ({})", key_type(other), display_value(lua, other)),
    };
    lua.create_string(text)
        .map(LuaValue::String)
        .map_err(|e| ScriptError(crate::runtime::ctx::lua_error_message(&e)))
}

/// A number's whole part as a 64-bit integer, what x86's conversion makes of
/// one too large for it included (observed for 1e20 and infinity).
fn whole(n: f64) -> i64 {
    if n.abs() < 9_223_372_036_854_775_808.0 {
        n as i64
    } else {
        i64::MIN
    }
}

/// The name Roblox's own variant types give a key's type.
fn key_type(key: &LuaValue) -> String {
    match type_name(key).as_str() {
        "table" => "Table".into(),
        "function" => "Function".into(),
        "CFrame" => "CoordinateFrame".into(),
        "BrickColor" => "int".into(),
        "EnumItem" => "token".into(),
        other => other.into(),
    }
}
