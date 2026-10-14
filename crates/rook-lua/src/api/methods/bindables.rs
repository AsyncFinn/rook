//! BindableEvent and BindableFunction.

use super::*;

pub(super) fn bindable_fire(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let args = a.into_vec();
    fire(lua, &SignalKey::event(this, "Event"), args);
    Outcome::ret(lua, ())
}

pub(super) fn bindable_invoke(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let callback = get_callback(lua, this, "OnInvoke")?.ok_or_else(|| {
        // Roblox waits for a callback to be set; Rook reports it instead
        // until callback queues exist.
        ScriptError("BindableFunction:Invoke has no OnInvoke callback".into())
    })?;
    Ok(Outcome::Call(callback, MultiValue::from_vec(a.into_vec())))
}

fn callbacks_table(lua: &Lua) -> mlua::Result<mlua::Table> {
    match lua.named_registry_value::<Option<mlua::Table>>("rook.callbacks")? {
        Some(t) => Ok(t),
        None => {
            let t = crate::ctx::weak_table(lua, "k")?;
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
    let ud = push_instance(lua, id)?;
    let all = callbacks_table(lua)?;
    let t: mlua::Table = match all.raw_get::<Option<mlua::Table>>(ud.clone())? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            all.raw_set(ud, t.clone())?;
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
    let ud = push_instance(lua, id)?;
    match callbacks_table(lua)?.raw_get::<Option<mlua::Table>>(ud)? {
        Some(t) => t.raw_get(name),
        None => Ok(None),
    }
}
