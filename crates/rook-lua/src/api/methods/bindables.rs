//! BindableEvent and BindableFunction.

use super::*;

mod copy;

use copy::copy_values;

/// Each listener gets its own copy, made now, not when it runs.
pub(super) fn bindable_fire(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let args = copy_values(lua, &a.rest(0))?;
    fire_with(lua, &SignalKey::event(this, "Event"), || {
        copy_values(lua, &args).unwrap_or_default()
    });
    Outcome::ret(lua, ())
}

/// `std.copy_bindable(...)`: what `Invoke`'s arguments and results become.
pub(crate) fn install(lua: &Lua, std: &mlua::Table) -> mlua::Result<()> {
    std.set(
        "copy_bindable",
        crate::ctx::api_fn(lua, |lua, args| {
            let copied = copy_values(lua, &args.into_vec())?;
            Ok(Outcome::Return(MultiValue::from_vec(copied)))
        })?,
    )
}

fn callbacks_table(lua: &Lua) -> mlua::Result<mlua::Table> {
    match lua.named_registry_value::<Option<mlua::Table>>("rook.callbacks")? {
        Some(t) => Ok(t),
        None => {
            // By instance key: a callback belongs to the instance, which
            // outlives the userdata it was set through.
            let t = lua.create_table()?;
            lua.set_named_registry_value("rook.callbacks", t.clone())?;
            Ok(t)
        }
    }
}

/// Drop `id`'s callbacks, as it's being freed.
pub(crate) fn forget_callbacks(lua: &Lua, id: InstanceId) -> mlua::Result<()> {
    match lua.named_registry_value::<Option<mlua::Table>>("rook.callbacks")? {
        Some(all) => all.raw_set(crate::api::instance::key_of(id), mlua::Value::Nil),
        None => Ok(()),
    }
}

/// Store a callback member (`OnInvoke`, `OnServerInvoke`, ...).
pub(crate) fn set_callback(
    lua: &Lua,
    id: InstanceId,
    name: &str,
    f: Option<Function>,
) -> mlua::Result<()> {
    let key = crate::api::instance::key_of(id);
    let all = callbacks_table(lua)?;
    let t: mlua::Table = match all.raw_get::<Option<mlua::Table>>(key)? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            all.raw_set(key, t.clone())?;
            t
        }
    };
    t.raw_set(name, f)
}

pub(crate) fn get_callback(
    lua: &Lua,
    id: InstanceId,
    name: &str,
) -> mlua::Result<Option<Function>> {
    let key = crate::api::instance::key_of(id);
    match callbacks_table(lua)?.raw_get::<Option<mlua::Table>>(key)? {
        Some(t) => t.raw_get(name),
        None => Ok(None),
    }
}

/// `Invoke`, written in Luau: it waits until a callback is set.
pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        ("BindableFunction", "Invoke") => {
            r#"
            local std = ...
            local wait_for_callback, copy = std.wait_for_callback, std.copy_bindable
            local function call(self, ...)
                return wait_for_callback(self, "OnInvoke")(...)
            end
            return function(self, ...)
                return copy(call(self, copy(...)))
            end
            "#
        }
        _ => return None,
    })
}
