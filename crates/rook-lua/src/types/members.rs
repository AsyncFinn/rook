//! Reading a member a datatype doesn't have raises an error, worded one of
//! two ways depending on the type (Studio 0.747, 2026-11-11, for every
//! datatype scripts can make). Scripts lean on it: `pcall(function()
//! return v.ClassName end)` is a common test for an Instance.

use mlua::{MetaMethod, UserData, UserDataMethods, Value as LuaValue};

use super::err;
use crate::ctx::args::type_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wording {
    /// `key is not a valid member of CFrame`; a key that isn't a string
    /// is a bad argument instead.
    Plain,
    /// `'key' is not a valid member of UDim`, whatever the key.
    Quoted,
}

/// The error for reading `key` from a value `owner` names.
pub(crate) fn not_a_member(key: &LuaValue, owner: &str, wording: Wording) -> mlua::Error {
    match (wording, key) {
        (Wording::Plain, LuaValue::String(s)) => err(format!(
            "{} is not a valid member of {owner}",
            s.to_string_lossy()
        )),
        (Wording::Plain, other) => err(format!(
            "invalid argument #2 (string expected, got {})",
            type_name(other)
        )),
        (Wording::Quoted, key) => err(format!(
            "'{}' is not a valid member of {owner}",
            key.to_string().unwrap_or_default()
        )),
    }
}

/// Make reading anything a `T` has no field or method for raise
/// [`not_a_member`]: Luau looks at fields and methods first.
pub(crate) fn add_unknown<T: UserData, M: UserDataMethods<T>>(
    m: &mut M,
    owner: &'static str,
    wording: Wording,
) {
    m.add_meta_method(MetaMethod::Index, move |_, _, key: LuaValue| {
        Err::<(), _>(not_a_member(&key, owner, wording))
    });
}
