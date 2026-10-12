//! PhysicsService collision groups, `BasePart:CanCollideWith` and
//! RocketPropulsion's Fire and Abort. Behavior and messages observed in
//! Studio 0.742; the groups themselves live on Workspace (see
//! `rook_physics::groups`).

use mlua::Lua;
use rook_dom::InstanceId;
use rook_physics::Command;
use rook_physics::groups::{self, CollisionGroups, GroupError, MAX_GROUPS};
use rook_types::Value;

use super::methods::{RawMethod, mutate};
use crate::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("PhysicsService", "RegisterCollisionGroup") => register,
        ("PhysicsService", "UnregisterCollisionGroup" | "RemoveCollisionGroup") => unregister,
        ("PhysicsService", "RenameCollisionGroup") => rename,
        ("PhysicsService", "CollisionGroupSetCollidable") => set_collidable,
        ("PhysicsService", "CollisionGroupsAreCollidable") => are_collidable,
        ("PhysicsService", "IsCollisionGroupRegistered") => is_registered,
        ("PhysicsService", "GetRegisteredCollisionGroups") => registered_groups,
        ("PhysicsService", "GetCollisionGroups") => legacy_groups,
        ("PhysicsService", "GetMaxCollisionGroups") => max_groups,
        ("PhysicsService", "CreateCollisionGroup") => create,
        ("PhysicsService", "GetCollisionGroupId") => group_id,
        ("PhysicsService", "GetCollisionGroupName") => group_name,
        ("PhysicsService", "SetPartCollisionGroup") => set_part_group,
        ("PhysicsService", "CollisionGroupContainsPart") => contains_part,
        ("BasePart", "CanCollideWith") => can_collide_with,
        ("RocketPropulsion", "Fire" | "fire") => fire_rocket,
        ("RocketPropulsion", "Abort") => abort_rocket,
        _ => return None,
    })
}

fn read(lua: &Lua) -> CollisionGroups {
    with_world(lua, |w| CollisionGroups::read(&w.dom))
}

/// Change the groups and save them on Workspace.
fn edit(
    lua: &Lua,
    f: impl FnOnce(&mut CollisionGroups) -> Result<(), GroupError>,
) -> ApiResult<()> {
    let mut groups = read(lua);
    f(&mut groups).map_err(|e| ScriptError(e.to_string()))?;
    mutate(lua, |w| groups.write(&mut w.dom))
}

fn register(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    edit(lua, |g| g.register(&name))?;
    Outcome::ret(lua, ())
}

fn unregister(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    edit(lua, |g| g.unregister(&name))?;
    Outcome::ret(lua, ())
}

fn rename(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let (from, to) = (a.string(0)?, a.string(1)?);
    edit(lua, |g| g.rename(&from, &to))?;
    Outcome::ret(lua, ())
}

fn set_collidable(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let (x, y, collides) = (a.string(0)?, a.string(1)?, a.truthy(2));
    edit(lua, |g| g.set_collidable(&x, &y, collides))?;
    Outcome::ret(lua, ())
}

fn are_collidable(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let (x, y) = (a.string(0)?, a.string(1)?);
    Outcome::ret(lua, read(lua).collidable(&x, &y))
}

fn is_registered(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    Outcome::ret(lua, read(lua).get(&name).is_some())
}

/// Tables of `name` and `mask`; the legacy `GetCollisionGroups` adds `id`.
fn group_tables(lua: &Lua, with_id: bool) -> ApiResult<Outcome> {
    let groups = read(lua);
    let list = lua.create_table()?;
    for (i, g) in groups.0.iter().enumerate() {
        let t = lua.create_table()?;
        t.set("name", g.name.as_str())?;
        t.set("mask", g.mask)?;
        if with_id {
            t.set("id", g.id)?;
        }
        list.raw_set(i + 1, t)?;
    }
    Outcome::ret(lua, list)
}

fn registered_groups(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    group_tables(lua, false)
}

fn legacy_groups(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    group_tables(lua, true)
}

fn max_groups(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, MAX_GROUPS)
}

/// Deprecated: registers the group and returns its id; the name must be
/// new.
fn create(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    if read(lua).get(&name).is_some() {
        return Err(ScriptError(GroupError::Exists.to_string()));
    }
    edit(lua, |g| g.register(&name))?;
    Outcome::ret(lua, read(lua).id_of(&name))
}

fn group_id(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let id = read(lua)
        .get(&name)
        .map(|g| g.id)
        .ok_or_else(|| ScriptError(GroupError::NotFound.to_string()))?;
    Outcome::ret(lua, id)
}

/// The name for an id, or an empty string.
fn group_name(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let id = a.number(0)?;
    let groups = read(lua);
    let name = groups
        .0
        .iter()
        .find(|g| f64::from(g.id) == id)
        .map_or("", |g| g.name.as_str());
    Outcome::ret(lua, name)
}

fn set_part_group(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let (part, name) = (a.instance(0)?, a.string(1)?);
    mutate(lua, |w| {
        w.dom
            .set_property_by_name(part, "CollisionGroup", Value::String(name))
    })?;
    Outcome::ret(lua, ())
}

fn contains_part(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let (name, part) = (a.string(0)?, a.instance(1)?);
    Outcome::ret(
        lua,
        with_world(lua, |w| groups::part_group(&w.dom, part) == name),
    )
}

/// Both parts collide and their groups collide (creator-docs
/// BasePart:CanCollideWith). Both must be in Workspace (observed).
fn can_collide_with(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let other = a.instance(0)?;
    let in_world = with_world(lua, |w| {
        w.dom
            .find_service("Workspace")
            .is_some_and(|ws| w.dom.is_ancestor_of(ws, this) && w.dom.is_ancestor_of(ws, other))
    });
    if !in_world {
        return Err(ScriptError(
            "CanCollideWith expects both parts to be descendants of the same WorldRoot.".into(),
        ));
    }
    let result = with_world(lua, |w| {
        let collides = |p| w.dom.property_by_name(p, "CanCollide") != Some(Value::Bool(false));
        let groups = CollisionGroups::read(&w.dom);
        collides(this)
            && collides(other)
            && groups.collidable(
                &groups::part_group(&w.dom, this),
                &groups::part_group(&w.dom, other),
            )
    });
    Outcome::ret(lua, result)
}

/// Starts the rocket flying toward its target at the next physics step.
fn fire_rocket(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    with_world(lua, |w| w.physics_commands.push(Command::FireRocket(this)));
    Outcome::ret(lua, ())
}

fn abort_rocket(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    with_world(lua, |w| w.physics_commands.push(Command::AbortRocket(this)));
    Outcome::ret(lua, ())
}
