//! BindableEvent and BindableFunction.

use super::*;

pub(super) fn bindable_fire(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let args = a.rest(0);
    fire(lua, &SignalKey::event(this, "Event"), &args);
    Outcome::ret(lua, ())
}

pub(super) fn bindable_invoke(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let callback = get_callback(lua, this, "OnInvoke")?.ok_or_else(|| {
        // Roblox waits for a callback to be set; Rook reports it instead
        // until callback queues exist.
        ScriptError("BindableFunction:Invoke has no OnInvoke callback".into())
    })?;
    Ok(Outcome::Call(callback, MultiValue::from_vec(a.rest(0))))
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
            local wait_for_callback = (...).wait_for_callback
            return function(self, ...)
                return wait_for_callback(self, "OnInvoke")(...)
            end
            "#
        }
        _ => return None,
    })
}
