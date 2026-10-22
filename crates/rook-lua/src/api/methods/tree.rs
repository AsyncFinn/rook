//! Object and Instance: the tree, finding, destroying, cloning.

use super::*;
use crate::removal;

/// What a method with a fast form found.
pub(crate) enum Found {
    Bool(bool),
    Instance(Option<InstanceId>),
    Instances(Vec<InstanceId>),
}

/// The DOM work of a method with a fast form: the instance, its string
/// argument and the truthiness of the one after.
pub(crate) type Core = fn(&World, InstanceId, &str, bool) -> Found;

/// Methods scripts call so often (a small place walks whole maps with them several
/// times a second) that they also have a fast form, which takes the
/// instance's key and the string argument (when `takes_string`) instead of
/// going through the general shim. Both forms run the same [`Core`]; the
/// general one still handles everything unusual (wrong argument types,
/// calls with `.`).
pub(crate) fn fast(owner: &str, name: &str) -> Option<(Core, bool)> {
    Some(match (owner, name) {
        ("Object", "IsA") => (is_a_core, true),
        ("Instance", "FindFirstChild") => (find_first_child_core, true),
        ("Instance", "GetChildren" | "children") => (children_core, false),
        _ => return None,
    })
}

fn ret_found(lua: &Lua, found: Found) -> ApiResult<Outcome> {
    match found {
        Found::Bool(b) => Outcome::ret(lua, b),
        Found::Instance(id) => ret_instance(lua, id),
        Found::Instances(ids) => ret_instances(lua, &ids),
    }
}

fn is_a_core(w: &World, this: InstanceId, class: &str, _: bool) -> Found {
    Found::Bool(w.dom.is_a(this, class))
}

pub(super) fn is_a(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let class = a.string(0)?;
    ret_found(lua, with_world(lua, |w| is_a_core(w, this, &class, false)))
}

pub(super) fn property_changed_signal(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let pid = with_world(lua, |w| w.dom.property_id(this, &name))
        .ok_or_else(|| ScriptError(format!("{name} is not a valid property name.")))?;
    Outcome::ret(
        lua,
        LSignal {
            instance: this,
            kind: SignalKind::Property(pid),
        },
    )
}

fn find_first_child_core(w: &World, this: InstanceId, name: &str, recursive: bool) -> Found {
    Found::Instance(w.dom.find_first_child(this, name, recursive))
}

pub(super) fn find_first_child(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let recursive = a.truthy(1);
    ret_found(
        lua,
        with_world(lua, |w| find_first_child_core(w, this, &name, recursive)),
    )
}

fn find_child_by(
    lua: &Lua,
    this: InstanceId,
    recursive: bool,
    pred: impl Fn(&World, InstanceId) -> bool,
) -> ApiResult<Outcome> {
    let found = with_world(lua, |w| {
        let w = &*w;
        if recursive {
            w.dom.find_descendant(this, &|c| pred(w, c))
        } else {
            w.dom.children(this).iter().copied().find(|c| pred(w, *c))
        }
    });
    ret_instance(lua, found)
}

pub(super) fn find_first_child_of_class(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let class = a.string(0)?;
    ret_instance(
        lua,
        with_world(lua, |w| w.dom.find_first_child_of_class(this, &class)),
    )
}

pub(super) fn find_first_child_which_is_a(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let class = a.string(0)?;
    find_child_by(lua, this, a.truthy(1), |w, c| w.dom.is_a(c, &class))
}

pub(super) fn find_first_descendant(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    find_child_by(lua, this, true, |w, c| w.dom.name(c) == name)
}

fn find_ancestor_by(
    lua: &Lua,
    this: InstanceId,
    pred: impl Fn(&World, InstanceId) -> bool,
) -> ApiResult<Outcome> {
    let found = with_world(lua, |w| {
        let ancestors: Vec<_> = w.dom.ancestors(this).collect();
        ancestors.into_iter().find(|a| pred(w, *a))
    });
    ret_instance(lua, found)
}

pub(super) fn find_first_ancestor(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    find_ancestor_by(lua, this, |w, c| w.dom.name(c) == name)
}

pub(super) fn find_first_ancestor_of_class(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let class = a.string(0)?;
    find_ancestor_by(lua, this, |w, c| w.dom.class_name(c) == class)
}

pub(super) fn find_first_ancestor_which_is_a(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let class = a.string(0)?;
    find_ancestor_by(lua, this, |w, c| w.dom.is_a(c, &class))
}

fn children_core(w: &World, this: InstanceId, _: &str, _: bool) -> Found {
    Found::Instances(w.dom.children(this).to_vec())
}

pub(super) fn get_children(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    ret_found(lua, with_world(lua, |w| children_core(w, this, "", false)))
}

pub(super) fn get_descendants(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    ret_instances(lua, &with_world(lua, |w| w.dom.descendants(this)))
}

pub(super) fn is_descendant_of(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let other = a.instance(0)?;
    Outcome::ret(lua, with_world(lua, |w| w.dom.is_ancestor_of(other, this)))
}

pub(super) fn is_ancestor_of(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let other = a.instance(0)?;
    Outcome::ret(lua, with_world(lua, |w| w.dom.is_ancestor_of(this, other)))
}

pub(super) fn destroy(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    removal::destroy(lua, this)?;
    Outcome::ret(lua, ())
}

/// Deprecated `Remove`: Parent to nil, then the same for each descendant
/// (creator-docs Instance:Remove). Unlike Destroy nothing is locked.
pub(super) fn remove(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    fn rec(lua: &Lua, id: InstanceId) -> Result<(), DomError> {
        removal::set_parent(lua, id, None)?;
        for child in with_world(lua, |w| w.dom.children(id).to_vec()) {
            rec(lua, child)?;
        }
        Ok(())
    }
    rec(lua, this)?;
    Outcome::ret(lua, ())
}

pub(super) fn clear_all_children(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    for child in with_world(lua, |w| w.dom.children(this).to_vec()) {
        // Locked children (services) are skipped, as in Roblox.
        if !with_world(lua, |w| w.dom.is_parent_locked(child)) {
            removal::destroy(lua, child)?;
        }
    }
    Outcome::ret(lua, ())
}

pub(super) fn clone(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let copy = mutate(lua, |w| Ok(w.dom.clone_instance(this)))?;
    ret_instance(lua, copy)
}

pub(super) fn get_full_name(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, with_world(lua, |w| w.dom.full_name(this)))
}
