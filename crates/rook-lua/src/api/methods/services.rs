//! Services and Lighting.

use super::*;

pub(super) fn get_service(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let _ = this;
    let id = mutate(lua, |w| w.dom.get_service(&name))?;
    ret_instance(lua, Some(id))
}

pub(super) fn find_service(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    ret_instance(lua, with_world(lua, |w| w.dom.find_service(&name)))
}

/// Stored for the engine to call at shutdown.
pub(super) fn bind_to_close(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let f = a.function(0)?;
    let list: mlua::Table =
        match lua.named_registry_value::<Option<mlua::Table>>("rook.bind_to_close")? {
            Some(t) => t,
            None => {
                let t = lua.create_table()?;
                lua.set_named_registry_value("rook.bind_to_close", t.clone())?;
                t
            }
        };
    list.raw_push(f)?;
    Outcome::ret(lua, ())
}

pub(super) fn debris_add_item(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let item = a.instance(0)?;
    // Default lifetime 10 seconds (creator-docs Debris:AddItem).
    let lifetime = a.number_or(1, 10.0)?;
    with_world(lua, |w| {
        w.scheduler
            .schedule(Queue::Task, lifetime, Job::Destroy(item));
    });
    Outcome::ret(lua, ())
}

pub(super) fn get_minutes_after_midnight(
    lua: &Lua,
    this: InstanceId,
    _: Args,
) -> ApiResult<Outcome> {
    let hours = with_world(lua, |w| w.dom.clock_time(this));
    Outcome::ret(lua, hours * 60.0)
}

pub(super) fn set_minutes_after_midnight(
    lua: &Lua,
    this: InstanceId,
    a: Args,
) -> ApiResult<Outcome> {
    let minutes = a.number(0)?;
    mutate(lua, |w| {
        crate::api::properties::set_lighting_hours(w, this, minutes / 60.0)
    })?;
    Outcome::ret(lua, ())
}
