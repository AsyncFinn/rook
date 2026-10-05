//! `Enum`, `Enum.X` and `Enum.X.Y`.

use mlua::{
    AnyUserData, Lua, MetaMethod, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value as LuaValue,
};
use rook_reflection::{EnumId, database};

use crate::ctx::ctx;

/// The `Enum` global.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LEnums;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LEnum(pub EnumId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LEnumItem {
    pub(crate) enum_id: EnumId,
    pub(crate) value: u32,
}

fn err(msg: String) -> mlua::Error {
    mlua::Error::RuntimeError(msg)
}

impl LEnumItem {
    fn name(&self) -> &'static str {
        database()
            .enum_info(self.enum_id)
            .items
            .iter()
            .find(|(_, v)| *v == self.value)
            .map_or("", |(n, _)| n)
    }
}

/// The unique userdata for an enum item.
pub(crate) fn push_item(lua: &Lua, enum_id: EnumId, value: u32) -> mlua::Result<LuaValue> {
    let c = ctx(lua);
    let key = (f64::from(enum_id.0) * 4_294_967_296.0) + f64::from(value);
    if let LuaValue::UserData(u) = c.enum_items.raw_get::<LuaValue>(key)? {
        return Ok(LuaValue::UserData(u));
    }
    let ud = lua.create_userdata(LEnumItem { enum_id, value })?;
    c.enum_items.raw_set(key, ud.clone())?;
    Ok(LuaValue::UserData(ud))
}

fn find_item(enum_id: EnumId, name: &str) -> Option<u32> {
    database()
        .enum_info(enum_id)
        .items
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
}

fn has_value(enum_id: EnumId, value: u32) -> bool {
    database()
        .enum_info(enum_id)
        .items
        .iter()
        .any(|(_, v)| *v == value)
}

/// Accept an EnumItem of the right enum, its number, or its name, as
/// Roblox does when assigning enum properties.
pub(crate) fn coerce(_lua: &Lua, v: &LuaValue, enum_id: EnumId) -> Result<u32, String> {
    let enum_name = database().enum_info(enum_id).name;
    match v {
        LuaValue::UserData(u) => match u.borrow::<LEnumItem>() {
            Ok(item) if item.enum_id == enum_id => Ok(item.value),
            Ok(item) => Err(format!(
                "Invalid value \"{}\" for enum {enum_name}",
                item.name()
            )),
            Err(_) => Err(format!(
                "EnumItem expected, got {}",
                crate::args::type_name(v)
            )),
        },
        LuaValue::String(s) => {
            let s = s.to_string_lossy();
            find_item(enum_id, &s)
                .ok_or_else(|| format!("Invalid value \"{s}\" for enum {enum_name}"))
        }
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            let n = crate::args::number_of(v).unwrap_or(-1.0);
            let value = n as u32;
            if n >= 0.0 && has_value(enum_id, value) {
                Ok(value)
            } else {
                Err(format!("Invalid value {n} for enum {enum_name}"))
            }
        }
        _ => Err(format!(
            "EnumItem expected, got {}",
            crate::args::type_name(v)
        )),
    }
}

/// The value of `v` if it names an item of the enum called `enum_name`.
pub(crate) fn item_value(lua: &Lua, v: &LuaValue, enum_name: &str) -> Option<u32> {
    let e = database().enum_by_name(enum_name)?;
    coerce(lua, v, e.id).ok()
}

impl UserData for LEnumItem {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "EnumItem");
        f.add_field_method_get("Name", |_, t| Ok(t.name()));
        f.add_field_method_get("Value", |_, t| Ok(t.value));
        f.add_field_method_get("EnumType", |_, t| Ok(LEnum(t.enum_id)));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("IsA", |_, t, name: String| {
            Ok(database().enum_info(t.enum_id).name == name)
        });
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LEnumItem>| {
            Ok(*t == *o)
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!(
                "Enum.{}.{}",
                database().enum_info(t.enum_id).name,
                t.name()
            ))
        });
    }
}

impl UserData for LEnum {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Enum");
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("GetEnumItems", |lua, t, ()| {
            let items: Vec<LuaValue> = database()
                .enum_info(t.0)
                .items
                .iter()
                .map(|(_, v)| push_item(lua, t.0, *v))
                .collect::<mlua::Result<_>>()?;
            lua.create_sequence_from(items)
        });
        m.add_method("FromName", |lua, t, name: String| {
            match find_item(t.0, &name) {
                Some(v) => push_item(lua, t.0, v),
                None => Ok(LuaValue::Nil),
            }
        });
        m.add_method("FromValue", |lua, t, value: u32| {
            if has_value(t.0, value) {
                push_item(lua, t.0, value)
            } else {
                Ok(LuaValue::Nil)
            }
        });
        m.add_meta_method(MetaMethod::Index, |lua, t, key: String| {
            match find_item(t.0, &key) {
                Some(v) => push_item(lua, t.0, v),
                None => Err(err(format!(
                    "{key} is not a valid member of \"Enum.{}\"",
                    database().enum_info(t.0).name
                ))),
            }
        });
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LEnum>| Ok(*t == *o));
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(database().enum_info(t.0).name)
        });
    }
}

impl UserData for LEnums {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Enums");
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("GetEnums", |lua, _, ()| {
            lua.create_sequence_from(database().enums().iter().map(|e| LEnum(e.id)))
        });
        m.add_meta_method(MetaMethod::Index, |lua, _, key: String| {
            match database().enum_by_name(&key) {
                Some(e) => lua_enum(lua, e.id),
                None => Err(err(format!("{key} is not a valid member of \"Enum\""))),
            }
        });
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Enums"));
    }
}

/// Enum userdata are cached so `Enum.Material == Enum.Material` by identity too.
fn lua_enum(lua: &Lua, id: EnumId) -> mlua::Result<AnyUserData> {
    let cache: mlua::Table = match lua.named_registry_value::<Option<mlua::Table>>("rook.enums")? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            lua.set_named_registry_value("rook.enums", t.clone())?;
            t
        }
    };
    if let Some(u) = cache.raw_get::<Option<AnyUserData>>(id.0)? {
        return Ok(u);
    }
    let u = lua.create_userdata(LEnum(id))?;
    cache.raw_set(id.0, u.clone())?;
    Ok(u)
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set("Enum", LEnums)?;
    Ok(())
}
