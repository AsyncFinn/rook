//! What a bindable's arguments and results arrive as: tables are copied,
//! once per listener, by rules of their own (measured in Studio 0.746:
//! `docs/behavior/scripting/bindables.md`).

use std::ffi::c_void;

use mlua::{Lua, Table, Value as LuaValue};

use crate::ctx::args::{display_value, type_name};

use super::ScriptError;

/// Copies of `values`. Fails on what a table can't carry.
pub(in crate::api::methods) fn copy_values(
    lua: &Lua,
    values: &[LuaValue],
) -> Result<Vec<LuaValue>, ScriptError> {
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
    let cast = |e: mlua::Error| ScriptError(crate::ctx::lua_error_message(&e));
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
        .map_err(|e| ScriptError(crate::ctx::lua_error_message(&e)))
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
