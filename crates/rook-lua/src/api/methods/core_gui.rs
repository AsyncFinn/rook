//! StarterGui's handles on the GUI Roblox draws itself: `SetCore` and
//! `GetCore`, which call what the CoreScripts registered, and
//! `SetCoreGuiEnabled`, which the CoreScripts follow through
//! `CoreGuiChangedSignal`. See docs/behavior/gui/core-scripts.md.

use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};
use rook_dom::InstanceId;

use super::RawMethod;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::signal::SignalKey;
use crate::threads::fire;
use crate::types::enums::{enum_item, item_name};

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("StarterGui", "RegisterSetCore") => register_set_core,
        ("StarterGui", "RegisterGetCore") => register_get_core,
        ("StarterGui", "SetCore") => set_core,
        ("StarterGui", "GetCore") => get_core,
        ("StarterGui", "SetCoreGuiEnabled") => set_core_gui_enabled,
        ("StarterGui", "GetCoreGuiEnabled") => get_core_gui_enabled,
        _ => return None,
    })
}

/// The functions the CoreScripts registered, by parameter name.
fn registered(lua: &Lua, which: &str) -> mlua::Result<Table> {
    let key = format!("rook.{which}");
    if let Some(t) = lua.named_registry_value::<Option<Table>>(&key)? {
        return Ok(t);
    }
    let t = lua.create_table()?;
    lua.set_named_registry_value(&key, t.clone())?;
    Ok(t)
}

fn register(lua: &Lua, which: &str, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let f = a.function(1)?;
    registered(lua, which)?.raw_set(name, f)?;
    Outcome::ret(lua, ())
}

fn register_set_core(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    register(lua, "set_core", a)
}

fn register_get_core(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    register(lua, "get_core", a)
}

/// Confirmed: creator-docs StarterGui:SetCore; a name no CoreScript
/// registered (yet) is an error with this text (Inference from the
/// CoreScripts' retry loops, which match on it).
fn set_core(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let Some(f) = registered(lua, "set_core")?.raw_get::<Option<Function>>(name.as_str())? else {
        return Err(ScriptError(format!(
            "SetCore: {name} has not been registered by the CoreScripts"
        )));
    };
    Ok(Outcome::Call(
        f,
        MultiValue::from_vec(vec![a.get(1).clone()]),
    ))
}

fn get_core(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let Some(f) = registered(lua, "get_core")?.raw_get::<Option<Function>>(name.as_str())? else {
        return Err(ScriptError(format!(
            "GetCore: {name} has not been registered by the CoreScripts"
        )));
    };
    Ok(Outcome::Call(f, MultiValue::new()))
}

/// Every `Enum.CoreGuiType` but `All`, which stands for all of them.
fn each_type() -> impl Iterator<Item = (&'static str, u32)> {
    let db = rook_reflection::database();
    db.enum_by_name("CoreGuiType")
        .into_iter()
        .flat_map(|e| e.items.iter().copied())
        .filter(|(name, _)| *name != "All")
}

fn set_core_gui_enabled(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let kind = item_name(lua, a.get(0), "CoreGuiType").ok_or_else(|| a.wrong(0, "CoreGuiType"))?;
    let enabled = a.truthy(1);
    with_world(lua, |w| {
        let types: Vec<&str> = if kind == "All" {
            each_type().map(|(n, _)| n).collect()
        } else {
            vec![kind]
        };
        for t in types {
            if enabled {
                w.core_gui_disabled.remove(t);
            } else {
                w.core_gui_disabled.insert(t);
            }
        }
    });
    let value = rook_reflection::database()
        .enum_by_name("CoreGuiType")
        .and_then(|e| e.items.iter().find(|(name, _)| *name == kind))
        .map_or(0, |(_, value)| *value);
    let args = [
        enum_item(lua, "CoreGuiType", value)?,
        LuaValue::Boolean(enabled),
    ];
    fire(lua, &SignalKey::event(this, "CoreGuiChangedSignal"), &args);
    Outcome::ret(lua, ())
}

/// `All` is enabled while every type is (Inference).
fn get_core_gui_enabled(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let kind = item_name(lua, a.get(0), "CoreGuiType").ok_or_else(|| a.wrong(0, "CoreGuiType"))?;
    let enabled = with_world(lua, |w| {
        if kind == "All" {
            w.core_gui_disabled.is_empty()
        } else {
            !w.core_gui_disabled.contains(kind)
        }
    });
    Outcome::ret(lua, enabled)
}
