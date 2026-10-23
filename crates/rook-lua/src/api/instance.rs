//! The Instance userdata: member lookup, property reads and writes.
//!
//! Lookup order for `inst.Key` (Roblox's): properties, then methods and
//! events, then children named `Key`. Unknown keys raise
//! `Key is not a valid member of Class "Full.Name"`.

use std::ffi::c_void;

use crate::signal::SignalKind;
use mlua::{
    AnyUserData, Function, LightUserData, Lua, MultiValue, UserData, UserDataFields,
    Value as LuaValue,
};
use rook_dom::InstanceId;
use rook_reflection::members::Security;
use rook_reflection::{
    ClassId, EnumId, PropertyFlags, PropertyId, PropertyType, Scriptability, database, members,
};
use rook_types::{InstanceRef, Value};
use rustc_hash::FxHashMap;

use super::{methods, properties, signal::LSignal};
use crate::ctx::args::{Args, display_value};
use crate::ctx::convert::{from_lua, to_lua};
use crate::ctx::{ApiResult, Ctx, Outcome, ScriptError, api_fn, ctx, with_world};
use crate::threads::dispatch;

mod access;
mod calls;
mod lookup;

use access::*;
use calls::*;
use lookup::*;

pub(crate) use access::{read_property, write_property};
pub(crate) use lookup::{Identity, MemberTable, get_member, runtime_properties};

/// A script's handle to an instance. There is one userdata per instance,
/// so `==`, `rawequal` and table keys behave as on Roblox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LuaInstance(pub InstanceId);

impl UserData for LuaInstance {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Instance");
        f.add_meta_field_with("__index", |lua| {
            lua.named_registry_value::<Function>("rook.instance.index")
        });
        f.add_meta_field_with("__newindex", |lua| {
            lua.named_registry_value::<Function>("rook.instance.newindex")
        });
        f.add_meta_field_with("__tostring", |lua| {
            lua.named_registry_value::<Function>("rook.instance.tostring")
        });
    }
}

fn key_of(id: InstanceId) -> LightUserData {
    LightUserData(id.to_ref().0 as usize as *mut c_void)
}

fn id_of(key: LightUserData) -> InstanceId {
    InstanceId::from_ref(InstanceRef(key.0 as usize as u64))
}

/// The userdata for `id`, created on first use.
pub(crate) fn push_instance(lua: &Lua, id: InstanceId) -> mlua::Result<AnyUserData> {
    let c = ctx(lua);
    let key = key_of(id);
    if let LuaValue::UserData(u) = c.instances.raw_get::<LuaValue>(key)? {
        return Ok(u);
    }
    let ud = lua.create_userdata(LuaInstance(id))?;
    c.instances.raw_set(key, ud.clone())?;
    c.instance_keys.raw_set(ud.clone(), key)?;
    let class = with_world(lua, |w| {
        w.dom.get(id).map(|i| (i.class, i.unknown_class.is_none()))
    });
    if let Some(class) = class {
        let cache = c.class_method_caches.borrow().get(&class).cloned();
        let cache = match cache {
            Some(t) => t,
            None => {
                let t = lua.create_table()?;
                c.class_method_caches.borrow_mut().insert(class, t.clone());
                t
            }
        };
        c.method_caches.raw_set(ud.clone(), cache)?;
    }
    Ok(ud)
}

/// Return an array of the userdata for `ids`. The array is filled in
/// Luau, where finding a userdata scripts already have is one table read
/// instead of a round trip through Rust per instance.
pub(crate) fn ret_instance_array(lua: &Lua, ids: &[InstanceId]) -> ApiResult<Outcome> {
    let keys = lua.create_sequence_from(ids.iter().map(|&id| key_of(id)))?;
    let fill: Function = lua.named_registry_value("rook.instance.fill_array")?;
    Ok(Outcome::Call(
        fill,
        MultiValue::from_iter([LuaValue::Table(keys)]),
    ))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    // `inst.Key` and `inst.Key = v` run more than any other engine code, so
    // they skip the general shim: the shim finds the instance's light
    // userdata key in Luau, the key arrives as a borrowed string, and an
    // error comes back as a second result. Each of these saves a conversion
    // that costs more than the member lookup itself. A method is the same
    // function for every instance of a class (and methods come before
    // children in the lookup order), so a third result tells the shim to
    // keep it in the class's method cache, which it reads first next time.
    let index = lua.create_function(|lua, (key, name): (LightUserData, mlua::LuaString)| {
        let name = name.to_str()?;
        profiling::scope!("Instance.__index", &name);
        Ok(match get_member(lua, id_of(key), &name) {
            Ok((v, method, fast)) => (v, None, method, fast),
            Err(e) => (LuaValue::Nil, Some(e.0), false, None),
        })
    })?;
    let newindex = lua.create_function(
        |lua, (key, name, value): (LightUserData, mlua::LuaString, LuaValue)| {
            let name = name.to_str()?;
            profiling::scope!("Instance.__newindex", &name);
            Ok(set_member(lua, id_of(key), &name, value).err().map(|e| e.0))
        },
    )?;
    // Keys that aren't strings (`inst[1]`) go the general way.
    let index_other = api_fn(lua, |lua, args| {
        let (id, key) = this_and_key(lua, &args)?;
        Outcome::ret(lua, get_member(lua, id, &key)?.0)
    })?;
    let newindex_other = api_fn(lua, |lua, args| {
        let (id, key) = this_and_key(lua, &args)?;
        let value = args.get(2).cloned().unwrap_or(LuaValue::Nil);
        set_member(lua, id, &key, value)?;
        Outcome::ret(lua, ())
    })?;
    let create = lua.create_function(|lua, key: LightUserData| push_instance(lua, id_of(key)))?;
    let fast_index = {
        let c = ctx(lua);
        lua.create_function(move |lua, (key, member): (LightUserData, u32)| {
            fast_read(lua, &c, id_of(key), member)
        })?
    };
    let (get, set): (Function, Function) = lua
        .load(include_str!("luau/instance_index.luau"))
        .set_name("=[rook] instance")
        .set_environment(builtins_env(lua, &["type", "error"])?)
        .call((
            ctx(lua).instance_keys.clone(),
            ctx(lua).method_caches.clone(),
            index,
            newindex,
            index_other,
            newindex_other,
            fast_index,
            lua.create_table()?,
            ctx(lua).instances.clone(),
            create.clone(),
            crate::ctx::weak_table(lua, "k")?,
        ))?;
    lua.set_named_registry_value("rook.instance.index", get)?;
    lua.set_named_registry_value("rook.instance.newindex", set)?;

    let tostring = lua.create_function(|lua, ud: AnyUserData| {
        let id = ud.borrow::<LuaInstance>()?.0;
        Ok(with_world(lua, |w| w.dom.name(id).to_owned()))
    })?;
    lua.set_named_registry_value("rook.instance.tostring", tostring)?;

    let fill_array: Function = lua
        .load(
            r#"
            local instances, create = ...
            return function(keys)
                for i = 1, #keys do
                    local key = keys[i]
                    keys[i] = instances[key] or create(key)
                end
                return keys
            end
            "#,
        )
        .set_name("=[rook] instance_array")
        .call((ctx(lua).instances.clone(), create.clone()))?;
    lua.set_named_registry_value("rook.instance.fill_array", fill_array)?;

    // Wrappers for methods with a fast form. Results come back as values,
    // an instance's key (1) or an array of keys (2).
    let fast_method: Function = lua
        .load(include_str!("luau/fast_method.luau"))
        .set_name("=[rook] fast_method")
        .set_environment(builtins_env(lua, &["type"])?)
        .call((
            ctx(lua).instance_keys.clone(),
            ctx(lua).instances.clone(),
            create,
        ))?;
    lua.set_named_registry_value("rook.instance.fast_method", fast_method)?;
    Ok(())
}

/// An environment holding only the named standard builtins, marked
/// `safeenv` so Luau runs them on its fast paths (`type(key)` in the shim
/// above runs on every `inst.Key`). Engine chunks never change their
/// globals, which is what `safeenv` assumes; game scripts keep the shared
/// environment.
fn builtins_env(lua: &Lua, names: &[&str]) -> mlua::Result<mlua::Table> {
    let env = lua.create_table()?;
    let globals = lua.globals();
    for name in names {
        env.raw_set(*name, globals.raw_get::<LuaValue>(*name)?)?;
    }
    env.set_safeenv(true);
    Ok(env)
}

/// The instance and key of an `__index`/`__newindex` call.
fn this_and_key(lua: &Lua, args: &MultiValue) -> ApiResult<(InstanceId, String)> {
    let id = match args.front() {
        Some(LuaValue::UserData(u)) => u.borrow::<LuaInstance>().map(|i| i.0).ok(),
        _ => None,
    }
    .ok_or_else(|| ScriptError("invalid instance".into()))?;
    let key = match args.get(1) {
        Some(LuaValue::String(s)) => s.to_string_lossy(),
        Some(other) => display_value(lua, other),
        None => "nil".into(),
    };
    Ok((id, key))
}

fn not_a_member(lua: &Lua, id: InstanceId, key: &str) -> ScriptError {
    with_world(lua, |w| {
        ScriptError(format!(
            "{key} is not a valid member of {} \"{}\"",
            w.dom.class_name(id),
            w.dom.full_name(id)
        ))
    })
}
