//! Content: what modern asset properties (`ImageContent`, `AudioContent`,
//! `MouseIconContent`...) hold, a URI or an object, and the global
//! `Content` table that makes one. Measured in Studio 0.747: conformance
//! scripting/content.

use mlua::{
    Lua, MetaMethod, MultiValue, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value as LuaValue,
};
use rbx_types::ContentType;
use rook_types::Content;

use super::enums::enum_item;
use super::err;
use crate::ctx::args::{display_value, format_number, number_of, type_name};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LContent(pub Content);

impl LContent {
    /// An empty URI is no content, as Studio shows it.
    pub(crate) fn new(c: Content) -> Self {
        match c.as_uri() {
            Some("") => Self(Content::none()),
            _ => Self(c),
        }
    }

    /// `Enum.ContentSourceType`'s value for this content.
    fn source_type(&self) -> (u32, &'static str) {
        match self.0.value() {
            ContentType::Uri(_) => (1, "Uri"),
            ContentType::Object(_) => (2, "Object"),
            _ => (0, "None"),
        }
    }

    /// The object this content holds: a file's object referents don't name
    /// Rook instances, and Rook makes none.
    fn object(&self) -> LuaValue {
        LuaValue::Nil
    }
}

const MEMBERS: [&str; 3] = ["SourceType", "Uri", "Object"];

impl UserData for LContent {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Content");
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(
            MetaMethod::Eq,
            |_, t, o: UserDataRef<LContent>| Ok(*t == *o),
        );
        m.add_meta_method(MetaMethod::ToString, |lua, t, ()| {
            let (_, source) = t.source_type();
            Ok(match t.0.value() {
                ContentType::Uri(uri) => format!("Content{{SourceType={source}, Uri={uri}}}"),
                ContentType::Object(_) => format!(
                    "Content{{SourceType={source}, Object={}}}",
                    display_value(lua, &t.object())
                ),
                _ => format!("Content{{SourceType={source}}}"),
            })
        });
        m.add_meta_method(MetaMethod::Index, |lua, t, key: LuaValue| {
            let LuaValue::String(key) = key else {
                return Err(err(format!(
                    "invalid argument #2 (string expected, got {})",
                    type_name(&key)
                )));
            };
            match &*key.to_str()? {
                "SourceType" => enum_item(lua, "ContentSourceType", t.source_type().0),
                "Uri" => Ok(match t.0.as_uri() {
                    Some(uri) => LuaValue::String(lua.create_string(uri)?),
                    None => LuaValue::Nil,
                }),
                "Object" => Ok(t.object()),
                other => Err(err(format!("{other} is not a valid member of Content"))),
            }
        });
        m.add_meta_method(
            MetaMethod::NewIndex,
            |_, _, (key, _): (mlua::LuaString, LuaValue)| -> mlua::Result<()> {
                let key = key.to_string_lossy();
                Err(err(if MEMBERS.contains(&key.as_str()) {
                    format!("{key} member of Content is read-only and cannot be assigned to")
                } else {
                    // Inference: unknown members read as they do on lookup.
                    format!("{key} is not a valid member of Content")
                }))
            },
        );
    }
}

/// The one argument a constructor takes: any other count is refused before
/// its type is looked at.
fn one_arg(args: MultiValue) -> mlua::Result<LuaValue> {
    let mut args = args.into_vec();
    if args.len() != 1 {
        return Err(err("Invalid number of arguments."));
    }
    Ok(args.remove(0))
}

fn wrong(name: &str, expected: &str, got: &LuaValue) -> mlua::Error {
    err(format!(
        "invalid argument #1 to '{name}' ({expected} expected, got {})",
        type_name(got)
    ))
}

/// `Content.fromAssetId`'s id: rounded half away from zero, then made a
/// 64-bit integer. Measured: 2^63 wraps to the lowest one and ids beyond
/// it become 0. Inference: anything past 2^63 is "beyond" (Studio gave 0
/// for 1e19, and no double lies between the two that a script could test
/// cheaply).
fn asset_id(n: f64) -> i64 {
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    let n = n.round();
    if n.abs() > LIMIT {
        0
    } else if n == LIMIT {
        i64::MIN
    } else {
        n as i64
    }
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let t = lua.create_table()?;
    t.set(
        "fromUri",
        lua.create_function(|_, args: MultiValue| {
            let v = one_arg(args)?;
            let uri = match &v {
                LuaValue::String(s) => s.to_string_lossy(),
                LuaValue::Integer(_) | LuaValue::Number(_) => {
                    format_number(number_of(&v).unwrap_or_default())
                }
                _ => return Err(wrong("fromUri", "string", &v)),
            };
            Ok(LContent::new(Content::from_uri(uri)))
        })?,
    )?;
    t.set(
        "fromAssetId",
        lua.create_function(|_, args: MultiValue| {
            let v = one_arg(args)?;
            let n = number_of(&v).ok_or_else(|| wrong("fromAssetId", "number", &v))?;
            if !n.is_finite() {
                return Err(err("Invalid id. Expected a finite value."));
            }
            Ok(LContent(Content::from_uri(format!(
                "rbxassetid://{}",
                asset_id(n)
            ))))
        })?,
    )?;
    t.set(
        "fromObject",
        lua.create_function(|_, args: MultiValue| -> mlua::Result<LContent> {
            let v = one_arg(args)?;
            // TODO: take EditableImage and EditableMesh once Rook has them.
            Err(match v {
                LuaValue::Nil => err("Expected Object, got nil."),
                v => wrong("fromObject", "Object", &v),
            })
        })?,
    )?;
    t.set("none", LContent(Content::none()))?;
    t.set_readonly(true);
    lua.globals().set("Content", t)
}

#[cfg(test)]
mod tests {
    use super::asset_id;

    #[test]
    fn asset_ids_round_and_wrap_as_studio() {
        assert_eq!(asset_id(4.5), 5);
        assert_eq!(asset_id(-0.5), -1);
        assert_eq!(asset_id(2f64.powi(63)), i64::MIN);
        assert_eq!(asset_id(-(2f64.powi(63))), i64::MIN);
        assert_eq!(asset_id(1e19), 0);
        assert_eq!(asset_id(-1e20), 0);
    }
}
