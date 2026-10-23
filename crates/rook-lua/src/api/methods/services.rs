//! Services and Lighting.

use super::*;

/// The DataModel's services, or those of `settings()` and
/// `UserSettings()` (made under them on first use, as the DataModel's
/// are).
pub(super) fn get_service(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let id = mutate(lua, |w| {
        if this == w.dom.root() {
            return w.dom.get_service(&name);
        }
        if let Some(found) = w.dom.find_first_child_of_class(this, &name) {
            return Ok(found);
        }
        let id = w.dom.create_by_name(&name)?;
        w.dom.set_name(id, name.clone())?;
        w.dom.set_parent(id, Some(this))?;
        Ok(id)
    })?;
    ret_instance(lua, Some(id))
}

pub(super) fn find_service(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    ret_instance(
        lua,
        with_world(lua, |w| {
            if this == w.dom.root() {
                w.dom.find_service(&name)
            } else {
                w.dom.find_first_child_of_class(this, &name)
            }
        }),
    )
}

/// The Teams under Teams, in order (creator-docs Teams:GetTeams).
pub(super) fn get_teams(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let teams = with_world(lua, |w| {
        w.dom
            .children(this)
            .iter()
            .copied()
            .filter(|&c| w.dom.is_a(c, "Team"))
            .collect::<Vec<_>>()
    });
    ret_instances(lua, &teams)
}

/// The policies for a player with no restrictions (creator-docs
/// PolicyService:GetPolicyInfoForPlayerAsync lists the keys): a local
/// session has no region or age to restrict by.
pub(super) fn policy_info(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let t = lua.create_table()?;
    t.set("AreAdsAllowed", true)?;
    t.set("ArePaidRandomItemsRestricted", false)?;
    t.set("IsContentSharingAllowed", true)?;
    t.set("IsEligibleToPurchaseSubscription", true)?;
    t.set("IsPaidItemTradingAllowed", true)?;
    t.set("IsSubjectToChinaPolicies", false)?;
    let links = [
        "Discord", "Facebook", "Twitch", "YouTube", "X", "GitHub", "Guilded",
    ];
    t.set(
        "AllowedExternalLinkReferences",
        lua.create_sequence_from(links)?,
    )?;
    Outcome::ret(lua, t)
}

/// Seconds since the Unix epoch on the clock every machine in a game
/// shares (creator-docs Workspace:GetServerTimeNow); one machine's clock
/// here.
pub(super) fn server_time_now(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    Outcome::ret(lua, now)
}

/// Stored for the engine to call at shutdown.
pub(super) fn bind_to_close(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
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

pub(super) fn debris_add_item(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
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
    _: &Args,
) -> ApiResult<Outcome> {
    let hours = with_world(lua, |w| w.dom.clock_time(this));
    Outcome::ret(lua, hours * 60.0)
}

pub(super) fn set_minutes_after_midnight(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let minutes = a.number(0)?;
    mutate(lua, |w| {
        crate::api::properties::set_lighting_hours(w, this, minutes / 60.0)
    })?;
    Outcome::ret(lua, ())
}
