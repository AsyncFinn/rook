//! StyleRules' properties (see `crate::styles` for how they apply).

use mlua::{Lua, Table, Value as LuaValue};
use rook_dom::InstanceId;

use super::RawMethod;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("StyleRule", "SetProperties") => set_properties,
        ("StyleRule", "SetProperty") => set_property,
        ("StyleRule", "GetProperties") => get_properties,
        ("StyleRule", "GetProperty") => get_property,
        ("Instance", "ResetPropertyToDefault") => reset_property,
        _ => return None,
    })
}

/// The rule's own table of properties, made when first set.
fn table_of(lua: &Lua, rule: InstanceId) -> mlua::Result<Table> {
    let found = with_world(lua, |w| {
        w.styles
            .properties
            .get(&rule)
            .map(|k| lua.registry_value::<Table>(k))
    });
    if let Some(t) = found {
        return t;
    }
    let t = lua.create_table()?;
    let key = lua.create_registry_value(t.clone())?;
    with_world(lua, |w| w.styles.properties.insert(rule, key));
    Ok(t)
}

/// Replaces every property the rule sets (creator-docs
/// StyleRule:SetProperties).
fn set_properties(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let LuaValue::Table(given) = a.get(0) else {
        return Err(a.wrong(0, "table"));
    };
    let t = lua.create_table()?;
    for pair in given.clone().pairs::<String, LuaValue>() {
        let (k, v) = pair?;
        t.raw_set(k, v)?;
    }
    let key = lua.create_registry_value(t)?;
    with_world(lua, |w| {
        w.styles.properties.insert(this, key);
        w.styles.invalidate();
    });
    Outcome::ret(lua, ())
}

fn set_property(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    table_of(lua, this)?.raw_set(name, a.get(1).clone())?;
    with_world(lua, |w| w.styles.invalidate());
    Outcome::ret(lua, ())
}

fn get_properties(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let copy = lua.create_table()?;
    for pair in table_of(lua, this)?.pairs::<LuaValue, LuaValue>() {
        let (k, v) = pair?;
        copy.raw_set(k, v)?;
    }
    Outcome::ret(lua, copy)
}

fn get_property(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    Outcome::ret(lua, table_of(lua, this)?.raw_get::<LuaValue>(name)?)
}

/// Confirmed: creator-docs Instance:ResetPropertyToDefault. The property
/// is unset, so styles may set it again: React clears a hover color this
/// way, and the row's StyleSheet value must come back at once.
fn reset_property(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    super::mutate(lua, |w| match w.dom.property_id(this, &name) {
        Some(p) => w.dom.reset_property(this, p),
        None => Ok(()),
    })?;
    with_world(lua, |w| w.styles.restyle_later(this));
    Outcome::ret(lua, ())
}
