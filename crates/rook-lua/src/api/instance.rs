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
use rook_reflection::{PropertyFlags, Scriptability, database, members};
use rook_signal::SignalKind;

use super::{methods, properties, signal::LSignal};
use crate::args::{Args, display_value};
use crate::convert::{from_lua, to_lua};
use crate::ctx::{ApiResult, Outcome, ScriptError, api_fn, ctx, with_world};
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

/// The userdata for `id`, created on first use.
pub(crate) fn push_instance(lua: &Lua, id: InstanceId) -> mlua::Result<AnyUserData> {
    let c = ctx(lua);
    let key = key_of(id);
    if let LuaValue::UserData(u) = c.instances.raw_get::<LuaValue>(key)? {
        return Ok(u);
    }
    let ud = lua.create_userdata(LuaInstance(id))?;
    c.instances.raw_set(key, ud.clone())?;
    Ok(ud)
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let index = api_fn(lua, |lua, args| {
        let (id, key) = this_and_key(lua, args)?;
        Outcome::ret(lua, get_member(lua, id, &key)?)
    })?;
    lua.set_named_registry_value("rook.instance.index", index)?;

    let newindex = api_fn(lua, |lua, args| {
        let mut v = args.into_vec();
        let value = if v.len() >= 3 {
            v.remove(2)
        } else {
            LuaValue::Nil
        };
        let (id, key) = this_and_key(lua, MultiValue::from_vec(v))?;
        set_member(lua, id, &key, value)?;
        Outcome::ret(lua, ())
    })?;
    lua.set_named_registry_value("rook.instance.newindex", newindex)?;

    let tostring = lua.create_function(|lua, ud: AnyUserData| {
        let id = ud.borrow::<LuaInstance>()?.0;
        Ok(with_world(lua, |w| w.dom.name(id).to_owned()))
    })?;
    lua.set_named_registry_value("rook.instance.tostring", tostring)?;
    Ok(())
}

fn this_and_key(lua: &Lua, args: MultiValue) -> ApiResult<(InstanceId, String)> {
    let v = args.into_vec();
    let id = match v.first() {
        Some(LuaValue::UserData(u)) => u.borrow::<LuaInstance>().map(|i| i.0).ok(),
        _ => None,
    }
    .ok_or_else(|| ScriptError("invalid instance".into()))?;
    let key = match v.get(1) {
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
struct PropInfo {
    name: &'static str,
    scriptability: Scriptability,
    flags: PropertyFlags,
    security: [Security; 2],
}

fn find_property(lua: &Lua, id: InstanceId, key: &str) -> Option<PropInfo> {
    let class = with_world(lua, |w| {
        let inst = w.dom.get(id)?;
        inst.unknown_class.is_none().then_some(inst.class)
    })?;
    let db = database();
    let p = db.find_property(class, key)?;
    // Aliases read and write their canonical property, but keep their own
    // scriptability (`size` is an alias that scripts can't use).
    let canonical = db.find_canonical_property(class, key).unwrap_or(p);
    Some(PropInfo {
        name: canonical.name,
        scriptability: p.scriptability,
        flags: p.flags,
        security: property_security(db.class(p.owner).name, p.name),
    })
}

/// `inst[key]` for scripts.
/// Runtime-only property values set by engine Luau code (for example
/// `Tween.TweenInfo`, whose type isn't a stored value type).
pub(crate) fn runtime_properties(lua: &Lua) -> mlua::Result<mlua::Table> {
    if let Some(t) = lua.named_registry_value::<Option<mlua::Table>>("rook.runtime_props")? {
        return Ok(t);
    }
    let t = crate::ctx::weak_table(lua, "k")?;
    lua.set_named_registry_value("rook.runtime_props", t.clone())?;
    Ok(t)
}

fn runtime_property(lua: &Lua, id: InstanceId, key: &str) -> mlua::Result<Option<LuaValue>> {
    let ud = push_instance(lua, id)?;
    match runtime_properties(lua)?.raw_get::<Option<mlua::Table>>(ud)? {
        Some(t) => Ok(t.raw_get::<Option<LuaValue>>(key)?),
        None => Ok(None),
    }
}

pub(crate) fn get_member(lua: &Lua, id: InstanceId, key: &str) -> ApiResult<LuaValue> {
    if let Some(v) = runtime_property(lua, id, key)? {
        return Ok(v);
    }
    if key == "ClassName" {
        let name = with_world(lua, |w| w.dom.class_name(id).to_owned());
        return Ok(LuaValue::String(lua.create_string(name)?));
    }
    if let Some(p) = find_property(lua, id, key)
        && p.scriptability != Scriptability::None
    {
        if !allowed(p.security[0]) {
            return Err(ScriptError(format!(
                "The current thread cannot access '{key}' (lacking capability {:?})",
                p.security[0]
            )));
        }
        if p.flags.write_only {
            return Err(ScriptError(format!("{key} is a write-only member")));
        }
        return read_property(lua, id, p.name);
    }
    if let Some(v) = member_function_or_event(lua, id, key)? {
        return Ok(v);
    }
    if callback_member(lua, id, key).is_some() {
        let class = with_world(lua, |w| w.dom.class_name(id).to_owned());
        return Err(ScriptError(format!(
            "{key} is a callback member of {class}; you can only set the callback value, get is not available"
        )));
    }
    let child = with_world(lua, |w| w.dom.find_first_child(id, key, false));
    if let Some(child) = child {
        return Ok(LuaValue::UserData(push_instance(lua, child)?));
    }
    Err(not_a_member(lua, id, key))
}

/// Read a property by canonical name, including derived ones.
pub(crate) fn read_property(lua: &Lua, id: InstanceId, name: &str) -> ApiResult<LuaValue> {
    let (value, enum_id) = with_world(lua, |w| {
        let class = w.dom.get(id).map(|i| i.class);
        let prop = class.and_then(|c| w.dom.database().find_canonical_property(c, name));
        let enum_id = prop.and_then(|p| match p.ty {
            rook_reflection::PropertyType::Enum(e) => Some(e),
            rook_reflection::PropertyType::Value(_) => None,
        });
        let value =
            properties::get(w, id, name).or_else(|| prop.and_then(|p| w.dom.property(id, p.id)));
        (value, enum_id)
    });
    match value {
        Some(v) => Ok(to_lua(lua, &v, enum_id)?),
        None => Ok(LuaValue::Nil),
    }
}

/// `inst[key] = value` for scripts.
pub(crate) fn set_member(lua: &Lua, id: InstanceId, key: &str, value: LuaValue) -> ApiResult<()> {
    let Some(p) = find_property(lua, id, key).filter(|p| p.scriptability != Scriptability::None)
    else {
        if let Some(name) = callback_member(lua, id, key) {
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
        return Err(not_a_member(lua, id, key));
    };
    if !allowed(p.security[1]) {
        return Err(ScriptError(format!(
            "The current thread cannot write '{key}' (lacking capability {:?})",
            p.security[1]
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

/// Methods are looked up along the class chain in the member list. Each
/// `(owner, name)` gets one function, created on first use.
fn member_function_or_event(lua: &Lua, id: InstanceId, key: &str) -> ApiResult<Option<LuaValue>> {
    let class = with_world(lua, |w| w.dom.get(id).map(|i| i.class));
    let Some(class) = class else {
        return Ok(None);
    };
    let db = database();
    for c in db.superclasses(class) {
        let Some(m) = members().classes.get(c.name) else {
            continue;
        };
        if let Some(f) = m.functions.iter().find(|f| f.name == key) {
            if !allowed(f.security) {
                return Err(ScriptError(format!(
                    "The current thread cannot call '{key}' (lacking capability {:?})",
                    f.security
                )));
            }
            return Ok(Some(LuaValue::Function(method_function(
                lua,
                c.name,
                f.name.as_str(),
                f.preferred.as_deref(),
            )?)));
        }
        if let Some(e) = m.events.iter().find(|e| e.name == key) {
            if !allowed(e.security) {
                return Err(ScriptError(format!(
                    "The current thread cannot access '{key}' (lacking capability {:?})",
                    e.security
                )));
            }
            // Deprecated lowercase spellings (`childAdded`) are the same signal.
            let lowercase = e.name.starts_with(|c: char| c.is_ascii_lowercase());
            let canonical = m
                .events
                .iter()
                .find(|o| lowercase && o.name != e.name && o.name.eq_ignore_ascii_case(&e.name))
                .map_or(e.name.as_str(), |o| o.name.as_str());
            let kind = SignalKind::Event(canonical);
            return Ok(Some(mlua::IntoLua::into_lua(
                LSignal { instance: id, kind },
                lua,
            )?));
        }
    }
    Ok(None)
}

/// The declared name of a callback member (`OnInvoke`, ...), if `key` is one.
fn callback_member(lua: &Lua, id: InstanceId, key: &str) -> Option<&'static str> {
    let class = with_world(lua, |w| w.dom.get(id).map(|i| i.class))?;
    database().superclasses(class).find_map(|c| {
        members()
            .classes
            .get(c.name)?
            .callbacks
            .iter()
            .find(|cb| cb.name == key && allowed(cb.security))
            .map(|cb| cb.name.as_str())
    })
}

fn method_function(
    lua: &Lua,
    owner: &'static str,
    name: &'static str,
    preferred: Option<&'static str>,
) -> mlua::Result<Function> {
    let c = ctx(lua);
    let key = (owner.to_owned(), name.to_owned());
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
    c.methods.borrow_mut().insert(key, f.clone());
    Ok(f)
}
