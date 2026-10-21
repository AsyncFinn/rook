//! Moving and destroying instances the way Roblox does for scripts: each
//! step's signals fire before the next step, so handlers see the tree as
//! it is then (conformance case `signals/destroy_order`). A dying meteor's
//! `DescendantRemoving` handler, say, still finds its emitters in place.

use mlua::Lua;
use rook_dom::{DomError, InstanceId};

use crate::ctx::with_world;
use crate::threads::dispatch;

/// Run `f` on the world and fire the signals it recorded.
fn step<R>(lua: &Lua, f: impl FnOnce(&mut rook_dom::Dom) -> R) -> R {
    let start = with_world(lua, |w| w.dom.journal().end());
    let result = with_world(lua, |w| f(&mut w.dom));
    dispatch(lua, start);
    result
}

/// Set `id`'s Parent: `DescendantRemoving` before the move, the rest after.
pub(crate) fn set_parent(
    lua: &Lua,
    id: InstanceId,
    parent: Option<InstanceId>,
) -> Result<(), DomError> {
    if !with_world(lua, |w| w.dom.check_parent(id, parent))? {
        return Ok(());
    }
    announce(lua, id, parent);
    step(lua, |dom| dom.move_announced(id, parent))
}

/// `Instance:Destroy()`: the Parent locks and `Destroying` fires, the
/// instance leaves its parent, then the children it had at the start are
/// destroyed, even one a handler moved elsewhere meanwhile.
pub(crate) fn destroy(lua: &Lua, id: InstanceId) -> Result<(), DomError> {
    let Some(children) = step(lua, |dom| dom.begin_destroy(id))? else {
        return Ok(());
    };
    announce(lua, id, None);
    step(lua, |dom| dom.detach_destroyed(id));
    for child in children {
        destroy(lua, child)?;
    }
    step(lua, |dom| dom.end_destroy(id));
    Ok(())
}

/// Fire `DescendantRemoving` for `id` and its descendants on the ancestors
/// they'll leave for `parent`, in pre-order. Each one's children are taken
/// after its handlers ran, so a child they moved out isn't announced.
fn announce(lua: &Lua, id: InstanceId, parent: Option<InstanceId>) {
    let leaving = with_world(lua, |w| w.dom.leaving_ancestors(id, parent));
    if leaving.is_empty() {
        return;
    }
    let mut pending = vec![id];
    while let Some(d) = pending.pop() {
        let inside = with_world(lua, |w| d == id || w.dom.is_ancestor_of(id, d));
        if !inside {
            continue;
        }
        step(lua, |dom| dom.note_removing(leaving.clone(), d));
        with_world(lua, |w| pending.extend(w.dom.children(d).iter().rev()));
    }
}
