//! The Instance userdata: member lookup, property reads and writes.
//!
//! Lookup order for `inst.Key` (Roblox's): properties, then methods and
//! events, then children named `Key`. Unknown keys raise
//! `Key is not a valid member of Class "Full.Name"`.

use std::ffi::c_void;

use mlua::{
    AnyUserData, Function, LightUserData, Lua, MultiValue, UserData, UserDataFields,
    Value as LuaValue,
};
use rook_dom::InstanceId;
use rook_reflection::members::Security;
use rook_reflection::{
    ClassId, EnumId, PropertyFlags, PropertyId, PropertyType, Scriptability, database, members,
};
use rook_signal::SignalKind;
use rook_types::{InstanceRef, Value};
use rustc_hash::FxHashMap;

use super::{methods, properties, signal::LSignal};
use crate::args::{Args, display_value};
use crate::convert::{from_lua, to_lua};
use crate::ctx::{ApiResult, Ctx, Outcome, ScriptError, api_fn, ctx, with_world};
use crate::threads::dispatch;

/// A script's handle to an instance. There is one userdata per instance,
/// so `==`, `rawequal` and table keys behave as on Roblox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LuaInstance(pub InstanceId);

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
        .load(
            r#"
            local keys, methods, index, newindex, index_other, newindex_other,
                fast_index, member_ids, instances, create = ...
            -- Keys some class has as a method. Property reads skip the
            -- per-instance cache lookup below, which costs a table miss
            -- per instance when a script reads thousands of them.
            local method_names = {}
            local function get(self, key)
                local k = keys[self]
                if k == nil or type(key) ~= "string" then
                    return index_other(self, key)
                end
                if method_names[key] then
                    local method = methods[self][key]
                    if method ~= nil then
                        return method
                    end
                end
                -- A plain stored property read before on this class:
                -- read it by id. Instances come back as their keys.
                local id = member_ids[key]
                if id ~= nil then
                    local value, tag = fast_index(k, id)
                    if tag == nil then
                        return value
                    elseif tag == 1 then
                        return instances[value] or create(value)
                    end
                end
                local value, err, is_method, fast_id = index(k, key)
                if err then
                    error(err, 0)
                end
                if is_method then
                    method_names[key] = true
                    methods[self][key] = value
                elseif fast_id ~= nil then
                    member_ids[key] = fast_id
                end
                return value
            end
            local function set(self, key, value)
                local k = keys[self]
                if k == nil or type(key) ~= "string" then
                    newindex_other(self, key, value)
                    return
                end
                local err = newindex(k, key, value)
                if err then
                    error(err, 0)
                end
            end
            return get, set
            "#,
        )
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
        .load(
            r#"
            local keys, instances, create = ...
            local function result(r, tag)
                if tag == 1 then
                    return instances[r] or create(r)
                elseif tag == 2 then
                    for i = 1, #r do
                        local key = r[i]
                        r[i] = instances[key] or create(key)
                    end
                end
                return r
            end
            return function(fast, general, takes_string)
                if takes_string then
                    return function(self, a, ...)
                        local k = keys[self]
                        if k ~= nil and type(a) == "string" then
                            return result(fast(k, a, (...)))
                        end
                        return general(self, a, ...)
                    end
                end
                return function(self, ...)
                    local k = keys[self]
                    if k ~= nil then
                        return result(fast(k))
                    end
                    return general(self, ...)
                end
            end
            "#,
        )
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

/// Read and write security of a property, from the member list.
fn property_security(owner: &str, name: &str) -> [Security; 2] {
    members()
        .classes
        .get(owner)
        .and_then(|c| c.property_security.get(name))
        .copied()
        .unwrap_or([Security::None; 2])
}

/// Game scripts run at the normal game identity: `None` security only.
fn allowed(sec: Security) -> bool {
    sec == Security::None
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PropInfo {
    /// The canonical property (aliases resolved).
    name: &'static str,
    id: PropertyId,
    enum_id: Option<EnumId>,
    scriptability: Scriptability,
    flags: PropertyFlags,
    security: [Security; 2],
}

/// What a key names on a class.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Member {
    /// A property scripts can see.
    Property(PropInfo),
    Method {
        owner: &'static str,
        name: &'static str,
        preferred: Option<&'static str>,
    },
    /// An event, by its canonical name.
    Event(&'static str),
    /// A callback (`OnInvoke`): scripts can set it but not read it.
    Callback(&'static str),
    /// A method or event the game identity can't reach.
    Denied {
        verb: &'static str,
        security: Security,
    },
}

/// Every member of a class by name, resolved the way Roblox looks keys up:
/// properties, then methods and events from the class up through its
/// superclasses, then callbacks.
pub(crate) type MemberTable = FxHashMap<&'static str, Member>;

/// Build a class's member table. `known` is false for instances of classes
/// the database doesn't have, which get no properties.
fn member_table(class: ClassId, known: bool) -> MemberTable {
    let db = database();
    let mut table = MemberTable::default();
    if known {
        for &pid in &db.class(class).all_properties {
            let name = db.property(pid).name;
            // The most derived declaration of a name wins.
            let Some(p) = db.find_property(class, name) else {
                continue;
            };
            if p.scriptability == Scriptability::None {
                continue;
            }
            // Aliases read and write their canonical property, but keep
            // their own scriptability (`size` is an alias scripts can't use).
            let canonical = db.find_canonical_property(class, name).unwrap_or(p);
            table.insert(
                name,
                Member::Property(PropInfo {
                    name: canonical.name,
                    id: canonical.id,
                    enum_id: match canonical.ty {
                        PropertyType::Enum(e) => Some(e),
                        PropertyType::Value(_) => None,
                    },
                    scriptability: p.scriptability,
                    flags: p.flags,
                    security: property_security(db.class(p.owner).name, p.name),
                }),
            );
        }
    }
    let chain = || {
        db.superclasses(class)
            .filter_map(|c| Some((c.name, members().classes.get(c.name)?)))
    };
    for (owner, m) in chain() {
        for f in &m.functions {
            table
                .entry(f.name.as_str())
                .or_insert(if allowed(f.security) {
                    Member::Method {
                        owner,
                        name: f.name.as_str(),
                        preferred: f.preferred.as_deref(),
                    }
                } else {
                    Member::Denied {
                        verb: "call",
                        security: f.security,
                    }
                });
        }
        for e in &m.events {
            let member = if allowed(e.security) {
                // Deprecated lowercase spellings (`childAdded`) are the same signal.
                let lowercase = e.name.starts_with(|c: char| c.is_ascii_lowercase());
                let canonical = m
                    .events
                    .iter()
                    .find(|o| lowercase && o.name != e.name && o.name.eq_ignore_ascii_case(&e.name))
                    .map_or(e.name.as_str(), |o| o.name.as_str());
                Member::Event(canonical)
            } else {
                Member::Denied {
                    verb: "access",
                    security: e.security,
                }
            };
            table.entry(e.name.as_str()).or_insert(member);
        }
    }
    for (_, m) in chain() {
        for cb in m.callbacks.iter().filter(|cb| allowed(cb.security)) {
            table
                .entry(cb.name.as_str())
                .or_insert(Member::Callback(cb.name.as_str()));
        }
    }
    table
}

/// What `key` names on an instance's class, if anything.
fn find_member(lua: &Lua, id: InstanceId, key: &str) -> Option<Member> {
    find_member_in(&ctx(lua), id, key).map(|(_, m)| m)
}

/// [`find_member`] with the context already fetched (fetching it costs
/// as much as a lookup, and `inst.Key` needs it several times).
/// Also gives the member table's own (static) copy of the key.
fn find_member_in(c: &Ctx, id: InstanceId, key: &str) -> Option<(&'static str, Member)> {
    let (class, known) = c
        .world
        .borrow()
        .dom
        .get(id)
        .map(|i| (i.class, i.unknown_class.is_none()))?;
    let found = |table: &MemberTable| table.get_key_value(key).map(|(k, m)| (*k, *m));
    if let Some(table) = c.member_tables.borrow().get(&(class, known)) {
        return found(table);
    }
    let table = member_table(class, known);
    let result = found(&table);
    c.member_tables.borrow_mut().insert((class, known), table);
    result
}

/// Runtime-only property values set by engine Luau code (for example
/// `Tween.TweenInfo`, whose type isn't a stored value type).
pub(crate) fn runtime_properties(lua: &Lua) -> mlua::Result<mlua::Table> {
    let c = ctx(lua);
    if let Some(t) = &*c.runtime_props.borrow() {
        return Ok(t.clone());
    }
    let t = crate::ctx::weak_table(lua, "k")?;
    *c.runtime_props.borrow_mut() = Some(t.clone());
    Ok(t)
}

fn runtime_property(
    lua: &Lua,
    c: &Ctx,
    id: InstanceId,
    key: &str,
) -> mlua::Result<Option<LuaValue>> {
    if !c.runtime_prop_ids.borrow().contains(&id) {
        return Ok(None);
    }
    let Some(all) = c.runtime_props.borrow().clone() else {
        return Ok(None);
    };
    match all.raw_get::<Option<mlua::Table>>(push_instance(lua, id)?)? {
        Some(t) => Ok(t.raw_get::<Option<LuaValue>>(key)?),
        None => Ok(None),
    }
}

/// `inst[key]` for scripts; whether it's a method (the same function for
/// every instance of the class, so the caller may cache it); and, for a
/// plain stored property, the member id [`fast_read`] reads it by.
pub(crate) fn get_member(
    lua: &Lua,
    id: InstanceId,
    key: &str,
) -> ApiResult<(LuaValue, bool, Option<u32>)> {
    let c = ctx(lua);
    if let Some(v) = runtime_property(lua, &c, id, key)? {
        return Ok((v, false, None));
    }
    if key == "ClassName" {
        let name = with_world(lua, |w| w.dom.class_name(id).to_owned());
        return Ok((LuaValue::String(lua.create_string(name)?), false, None));
    }
    let found = find_member_in(&c, id, key);
    let member = found.map(|(_, m)| m);
    let method = matches!(member, Some(Member::Method { .. }));
    let mut fast = None;
    let value = match member {
        Some(Member::Property(p)) => {
            if !allowed(p.security[0]) {
                return Err(ScriptError(format!(
                    "The current thread cannot access '{key}' (lacking capability {})",
                    p.security[0].capability()
                )));
            }
            if p.flags.write_only {
                return Err(ScriptError(format!("{key} is a write-only member")));
            }
            let (value, plain) = read(lua, &c, id, p.name, Some((p.id, p.enum_id)))?;
            if plain && let Some((static_key, _)) = found {
                fast = Some(register_fast_read(&c, id, static_key, p));
            }
            Ok(value)
        }
        Some(Member::Method {
            owner,
            name,
            preferred,
        }) => Ok(LuaValue::Function(method_function(
            lua, owner, name, preferred,
        )?)),
        Some(Member::Event(name)) => Ok(mlua::IntoLua::into_lua(
            LSignal {
                instance: id,
                kind: SignalKind::Event(name),
            },
            lua,
        )?),
        Some(Member::Denied { verb, security }) => Err(ScriptError(format!(
            "The current thread cannot {verb} '{key}' (lacking capability {})",
            security.capability()
        ))),
        Some(Member::Callback(_)) => {
            let class = with_world(lua, |w| w.dom.class_name(id).to_owned());
            Err(ScriptError(format!(
                "{key} is a callback member of {class}; you can only set the callback value, get is not available"
            )))
        }
        None => {
            let child = with_world(lua, |w| w.dom.find_first_child(id, key, false));
            match child {
                Some(child) => Ok(LuaValue::UserData(push_instance(lua, child)?)),
                None => Err(not_a_member(lua, id, key)),
            }
        }
    }?;
    Ok((value, method, fast))
}

/// Remember that reading `key` on `id`'s class is a plain read of `p`,
/// and give `key` its member id. Derived properties (`properties::get`)
/// depend only on the class, so what holds for one instance holds for
/// every instance of its class.
fn register_fast_read(c: &Ctx, id: InstanceId, key: &'static str, p: PropInfo) -> u32 {
    let member = {
        let mut ids = c.member_ids.borrow_mut();
        let next = ids.len() as u32;
        *ids.entry(key).or_insert(next)
    };
    if let Some(class) = c.world.borrow().dom.get(id).map(|i| i.class) {
        c.fast_reads
            .borrow_mut()
            .insert((class, member), (p.id, p.enum_id));
    }
    member
}

/// `inst.Key` by member id, for keys [`get_member`] found to be plain
/// stored properties of the instance's class. The second result is `None`
/// for a value, 1 when the value is an instance's key (the shim turns it
/// into the instance's userdata without a trip through Rust), and 0 when
/// the read must take the general path: another class, an unknown class,
/// or an instance with runtime-only values.
fn fast_read(
    lua: &Lua,
    c: &Ctx,
    id: InstanceId,
    member: u32,
) -> mlua::Result<(LuaValue, Option<u8>)> {
    const SLOW: (LuaValue, Option<u8>) = (LuaValue::Nil, Some(0));
    let value = {
        let w = c.world.borrow();
        let Some(inst) = w.dom.get(id).filter(|i| i.unknown_class.is_none()) else {
            return Ok(SLOW);
        };
        let Some(&(pid, enum_id)) = c.fast_reads.borrow().get(&(inst.class, member)) else {
            return Ok(SLOW);
        };
        if c.runtime_prop_ids.borrow().contains(&id) {
            return Ok(SLOW);
        }
        (w.dom.property(id, pid), enum_id)
    };
    Ok(match value {
        (Some(Value::Ref(Some(r))), _) => (
            LuaValue::LightUserData(key_of(InstanceId::from_ref(r))),
            Some(1),
        ),
        (Some(v), enum_id) => (to_lua(lua, &v, enum_id)?, None),
        (None, _) => (LuaValue::Nil, None),
    })
}

/// Read a property by canonical name, including derived ones.
pub(crate) fn read_property(lua: &Lua, id: InstanceId, name: &str) -> ApiResult<LuaValue> {
    let prop = with_world(lua, |w| {
        let class = w.dom.get(id)?.class;
        let p = w.dom.database().find_canonical_property(class, name)?;
        let enum_id = match p.ty {
            PropertyType::Enum(e) => Some(e),
            PropertyType::Value(_) => None,
        };
        Some((p.id, enum_id))
    });
    Ok(read(lua, &ctx(lua), id, name, prop)?.0)
}

/// Read a property given its canonical name and, when the database has
/// it, its ID and enum. Also says whether this was a plain read of the
/// stored value (not a derived property or a view of another).
fn read(
    lua: &Lua,
    c: &Ctx,
    id: InstanceId,
    name: &str,
    prop: Option<(PropertyId, Option<EnumId>)>,
) -> ApiResult<(LuaValue, bool)> {
    let (value, plain) = {
        let w = c.world.borrow();
        match prop.and_then(|(pid, _)| properties::content_view(pid)) {
            Some(target) => (
                w.dom.property(id, target).map(|v| {
                    Value::ContentId(v.content_url().unwrap_or_default().to_owned().into())
                }),
                false,
            ),
            None => match properties::get(&w, id, name) {
                Some(v) => (Some(v), false),
                None => (
                    prop.and_then(|(pid, _)| w.dom.property(id, pid)),
                    prop.is_some(),
                ),
            },
        }
    };
    let value = match value {
        Some(v) => to_lua(lua, &v, prop.and_then(|(_, e)| e))?,
        None => LuaValue::Nil,
    };
    Ok((value, plain))
}

/// `inst[key] = value` for scripts.
pub(crate) fn set_member(lua: &Lua, id: InstanceId, key: &str, value: LuaValue) -> ApiResult<()> {
    let p = match find_member(lua, id, key) {
        Some(Member::Property(p)) => p,
        Some(Member::Callback(name)) => {
            let f = match value {
                LuaValue::Function(f) => Some(f),
                LuaValue::Nil => None,
                _ => {
                    return Err(ScriptError(format!(
                        "Unable to assign property {key}. Function expected, got {}",
                        crate::args::type_name(&value)
                    )));
                }
            };
            methods::set_callback(lua, id, name, f)?;
            return Ok(());
        }
        _ => return Err(not_a_member(lua, id, key)),
    };
    if !allowed(p.security[1]) {
        return Err(ScriptError(format!(
            "The current thread cannot write '{key}' (lacking capability {})",
            p.security[1].capability()
        )));
    }
    if p.flags.read_only || p.scriptability == Scriptability::Read {
        return Err(ScriptError(format!(
            "Unable to assign property {key}. Property is read only"
        )));
    }
    write_property(lua, id, p.name, &value)
        .map_err(|e| ScriptError(format!("Unable to assign property {key}. {e}")))
        .and_then(|r| r)
}

/// Convert and store a property, then fire its signals. The outer error is
/// a conversion failure (gets the "Unable to assign" prefix); the inner one
/// comes from the DOM (locked parent, cycles) and is reported as is.
pub(crate) fn write_property(
    lua: &Lua,
    id: InstanceId,
    name: &str,
    value: &LuaValue,
) -> Result<ApiResult<()>, String> {
    let db = database();
    let (class, start) = with_world(lua, |w| {
        (w.dom.get(id).map(|i| i.class), w.dom.journal().end())
    });
    let Some(class) = class else {
        return Ok(Err(ScriptError("instance no longer exists".into())));
    };
    let prop = db
        .find_canonical_property(class, name)
        .ok_or_else(|| format!("{name} is not a property"))?;
    let converted = from_lua(lua, value, prop.ty)?;
    let result = with_world(lua, |w| {
        if let (Some(target), Some(url)) =
            (properties::content_view(prop.id), converted.content_url())
        {
            // A Decal's or Texture's image set to a web URL is stored as its
            // asset id (`rook_content::image_url_rewrite`).
            let rewritten = (matches!(name, "Texture" | "ColorMap") && w.dom.is_a(id, "Decal"))
                .then(|| rook_content::image_url_rewrite(url))
                .flatten();
            let url = rewritten.as_deref().unwrap_or(url);
            // A SpecialMesh given a mesh or a texture becomes a FileMesh
            // (observed in Studio 0.742; conformance case `content/decal`).
            if matches!(name, "MeshId" | "TextureId")
                && !url.is_empty()
                && w.dom.class_name(id) == "SpecialMesh"
            {
                w.dom
                    .set_property_by_name(id, "MeshType", rook_types::Value::Enum(5))?;
            }
            return w
                .dom
                .set_property(id, target, properties::content_value(url));
        }
        if let Some(r) = properties::set(w, id, name, &converted) {
            return r;
        }
        match name {
            "Name" => {
                let rook_types::Value::String(s) = converted else {
                    return Ok(());
                };
                w.dom.set_name(id, s)
            }
            "Parent" => {
                let rook_types::Value::Ref(r) = converted else {
                    return Ok(());
                };
                w.dom.set_parent(id, r.map(InstanceId::from_ref))
            }
            _ => w.dom.set_property(id, prop.id, converted),
        }
    });
    dispatch(lua, start);
    Ok(result.map_err(ScriptError::from))
}

/// Each `(owner, name)` method gets one function, created on first use,
/// so `a.Destroy == b.Destroy`.
fn method_function(
    lua: &Lua,
    owner: &'static str,
    name: &'static str,
    preferred: Option<&'static str>,
) -> mlua::Result<Function> {
    let c = ctx(lua);
    let key = (owner, name);
    if let Some(f) = c.methods.borrow().get(&key) {
        return Ok(f.clone());
    }
    if let Some(source) = methods::luau_source(owner, name) {
        let std: mlua::Table = lua.named_registry_value("rook.std")?;
        let f: Function = lua
            .load(source)
            .set_name(format!("=[rook] {owner}.{name}"))
            .call(std)?;
        c.methods.borrow_mut().insert(key, f.clone());
        return Ok(f);
    }
    // A deprecated lowercase spelling (`findFirstChild`, `makeJoints`)
    // behaves as the capitalized member.
    let capitalized = preferred
        .filter(|p| p.eq_ignore_ascii_case(name))
        .map(str::to_owned)
        .or_else(|| {
            let mut chars = name.chars();
            let first = chars.next()?;
            first
                .is_ascii_lowercase()
                .then(|| first.to_ascii_uppercase().to_string() + chars.as_str())
        });
    let resolved = if methods::native(owner, name).is_some() {
        name.to_owned()
    } else {
        capitalized.unwrap_or_else(|| name.to_owned())
    };
    let native = methods::native(owner, &resolved);
    let inert = native.is_some() && methods::is_inert(owner, &resolved);
    let resolved: &'static str = Box::leak(resolved.into_boxed_str());
    let f = match native {
        Some(raw) => api_fn(lua, move |lua, args| {
            profiling::scope!("Instance method", &format!("{owner}:{resolved}"));
            let (this, args) = Args::method(args, name)?;
            if inert {
                with_world(lua, |w| w.count_inert(format!("{owner}.{resolved}")));
            }
            raw(lua, this, args)
        })?,
        None => api_fn(lua, move |lua, args| {
            let (this, _) = Args::method(args, name)?;
            let class = with_world(lua, |w| {
                w.count_unimplemented(format!("{owner}.{name}"));
                w.dom.class_name(this).to_owned()
            });
            Err(ScriptError(format!(
                "{class}:{name} is not implemented in Rook yet"
            )))
        })?,
    };
    let f = match methods::fast(owner, resolved) {
        Some((core, takes_string)) => fast_method(lua, core, takes_string, f)?,
        None => f,
    };
    c.methods.borrow_mut().insert(key, f.clone());
    Ok(f)
}

/// A method with a fast form (see [`methods::fast`]): a Luau wrapper calls
/// `core` with the instance's key, and `general` for anything else.
fn fast_method(
    lua: &Lua,
    core: methods::Core,
    takes_string: bool,
    general: Function,
) -> mlua::Result<Function> {
    let c = ctx(lua);
    let fast = lua.create_function(
        move |lua, (key, arg, flag): (LightUserData, Option<String>, LuaValue)| {
            let found = core(
                &c.world.borrow(),
                id_of(key),
                arg.as_deref().unwrap_or_default(),
                !matches!(flag, LuaValue::Nil | LuaValue::Boolean(false)),
            );
            Ok(match found {
                methods::Found::Bool(b) => (LuaValue::Boolean(b), None),
                methods::Found::Instance(Some(id)) => {
                    (LuaValue::LightUserData(key_of(id)), Some(1))
                }
                methods::Found::Instance(None) => (LuaValue::Nil, None),
                methods::Found::Instances(ids) => (
                    LuaValue::Table(lua.create_sequence_from(ids.into_iter().map(key_of))?),
                    Some(2),
                ),
            })
        },
    )?;
    let wrap: Function = lua.named_registry_value("rook.instance.fast_method")?;
    wrap.call((fast, general, takes_string))
}
