//! DataStoreService, in memory: stores, and their methods written in Luau.

use mlua::{Lua, Table, Value as LuaValue};
use rook_dom::InstanceId;

use super::RawMethod;
use crate::api::instance::push_instance;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("DataStoreService", "GetDataStore") => get_data_store,
        ("DataStoreService", "GetGlobalDataStore") => get_global_data_store,
        ("DataStoreService", "GetOrderedDataStore") => get_ordered_data_store,
        ("DataStoreService", "GetRequestBudgetForRequestType") => request_budget,
        _ => return None,
    })
}

/// The stores' methods, written in Luau over the `std.datastore` helpers.
pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        ("GlobalDataStore", "GetAsync") => {
            r#"
            local std = ...
            local ds = std.datastore
            return function(self, key)
                return ds.copy(ds.data(self)[ds.key(key)])
            end
            "#
        }
        ("GlobalDataStore", "SetAsync") => {
            r#"
            local std = ...
            local ds = std.datastore
            return function(self, key, value)
                ds.data(self)[ds.key(key)] = ds.copy(value, true)
            end
            "#
        }
        ("GlobalDataStore", "UpdateAsync") => {
            r#"
            local std = ...
            local ds = std.datastore
            return function(self, key, transform)
                local k = ds.key(key)
                local data = ds.data(self)
                local new = transform(ds.copy(data[k]))
                if new ~= nil then
                    data[k] = ds.copy(new, true)
                end
                return ds.copy(data[k])
            end
            "#
        }
        ("GlobalDataStore", "IncrementAsync") => {
            r#"
            local std = ...
            local ds = std.datastore
            return function(self, key, delta)
                local k = ds.key(key)
                local data = ds.data(self)
                local value = (data[k] or 0) + (delta or 1)
                data[k] = value
                return value
            end
            "#
        }
        ("GlobalDataStore", "RemoveAsync") => {
            r#"
            local std = ...
            local ds = std.datastore
            return function(self, key)
                local k = ds.key(key)
                local data = ds.data(self)
                local old = data[k]
                data[k] = nil
                return old
            end
            "#
        }
        // Pages are plain tables here (Approximation: Roblox returns a
        // DataStorePages instance; scripts use the same members).
        ("OrderedDataStore", "GetSortedAsync") => {
            include_str!("../luau/get_sorted_async.luau")
        }
        _ => return None,
    })
}

/// Shared helpers for the DataStore sources, built once.
pub(crate) fn datastore_helpers(lua: &Lua) -> mlua::Result<Table> {
    lua.load(include_str!("../luau/datastore.luau"))
        .set_name("=[rook] DataStore")
        .call((
            lua.globals().get::<LuaValue>("error")?,
            lua.globals().get::<LuaValue>("type")?,
            lua.globals().get::<LuaValue>("typeof")?,
            lua.globals().get::<LuaValue>("tostring")?,
            lua.globals().get::<LuaValue>("pairs")?,
            lua.globals().get::<LuaValue>("setmetatable")?,
        ))
}

/// One store object per (kind, name, scope), as Roblox returns the same
/// object for repeated requests. Contents live in memory for this run
/// (Approximation: Rook has no persistent data stores yet).
fn store(lua: &Lua, class: &str, name: &str, scope: &str) -> ApiResult<Outcome> {
    let cache: Table = match lua.named_registry_value::<Option<Table>>("rook.datastores")? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            lua.set_named_registry_value("rook.datastores", t.clone())?;
            t
        }
    };
    let key = format!("{class}\0{name}\0{scope}");
    if let Some(u) = cache.raw_get::<Option<mlua::AnyUserData>>(key.as_str())? {
        return Outcome::ret(lua, u);
    }
    let id = with_world(lua, |w| {
        let id = w.dom.create_by_name(class).ok()?;
        w.dom.set_name(id, name.to_owned()).ok()?;
        Some(id)
    })
    .ok_or_else(|| ScriptError(format!("{class} is not available")))?;
    let ud = push_instance(lua, id)?;
    cache.raw_set(key, ud.clone())?;
    Outcome::ret(lua, ud)
}

fn get_data_store(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let scope = a.opt_string(1)?.unwrap_or_else(|| "global".into());
    store(lua, "DataStore", &name, &scope)
}

fn get_global_data_store(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    store(lua, "GlobalDataStore", "GlobalDataStore", "global")
}

fn get_ordered_data_store(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let scope = a.opt_string(1)?.unwrap_or_else(|| "global".into());
    store(lua, "OrderedDataStore", &name, &scope)
}

/// No request limits locally.
fn request_budget(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, 60)
}
