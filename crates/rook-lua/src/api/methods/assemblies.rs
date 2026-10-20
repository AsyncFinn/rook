//! Joints and assemblies (observed in Studio 0.741).

use super::models::parts_of;
use super::*;

/// Active JointInstances and WeldConstraints attached to the part.
pub(super) fn get_joints(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let ids: Vec<_> = with_world(lua, |w| {
        joints::part_joints(&w.dom, this)
            .iter()
            .map(|j| j.id)
            .collect()
    });
    ret_instances(lua, &ids)
}

pub(super) fn get_connected_parts(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let recursive = a.truthy(0);
    ret_instances(
        lua,
        &with_world(lua, |w| assembly::connected_parts(&w.dom, this, recursive)),
    )
}

pub(super) fn get_root_part(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    ret_instance(
        lua,
        with_world(lua, |w| Assembly::of(&w.dom, this).map(|a| a.root())),
    )
}

/// Whether the part's own assembly is anchored; a hinge to an anchored
/// assembly doesn't count.
pub(super) fn is_grounded(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let grounded = with_world(lua, |w| {
        Assembly::of(&w.dom, this).is_some_and(|a| a.anchored)
    });
    Outcome::ret(lua, grounded)
}

/// Parts in `ids` and their descendants, in order.
fn parts_in(w: &World, ids: &[InstanceId]) -> Vec<InstanceId> {
    let mut parts = Vec::new();
    for &id in ids {
        if w.dom.is_a(id, "BasePart") {
            parts.push(id);
        }
        parts.extend(parts_of(w, id));
    }
    parts
}

fn make_joints(lua: &Lua, parts: impl FnOnce(&World) -> Vec<InstanceId>) -> ApiResult<Outcome> {
    mutate(lua, |w| {
        let parts = parts(w);
        surfaces::make_joints(&mut w.dom, &parts)
    })?;
    Outcome::ret(lua, ())
}

/// Destroys every joint `GetJoints` lists for each part, WeldConstraints
/// included.
fn break_joints(lua: &Lua, parts: impl FnOnce(&World) -> Vec<InstanceId>) -> ApiResult<Outcome> {
    mutate(lua, |w| {
        for part in parts(w) {
            for j in joints::part_joints(&w.dom, part) {
                w.dom.destroy(j.id)?;
            }
        }
        Ok(())
    })?;
    Outcome::ret(lua, ())
}

pub(super) fn part_make_joints(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    make_joints(lua, |_| vec![this])
}

pub(super) fn part_break_joints(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    break_joints(lua, |_| vec![this])
}

pub(super) fn model_make_joints(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    make_joints(lua, |w| parts_of(w, this))
}

pub(super) fn model_break_joints(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    break_joints(lua, |w| parts_of(w, this))
}

/// Deprecated `Workspace:MakeJoints(objects)`: the parts among `objects`
/// and their descendants.
pub(super) fn workspace_make_joints(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let objects = a.instances(0)?;
    make_joints(lua, |w| parts_in(w, &objects))
}

pub(super) fn workspace_break_joints(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let objects = a.instances(0)?;
    break_joints(lua, |w| parts_in(w, &objects))
}

pub(super) fn set_desired_angle(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let angle = a.number(0)?;
    mutate(lua, |w| {
        w.dom
            .set_property_by_name(this, "DesiredAngle", Value::Float32(angle as f32))
    })?;
    Outcome::ret(lua, ())
}
