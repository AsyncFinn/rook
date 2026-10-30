//! What the `inst.Key` shim and `GetChildren` keep per instance (its name,
//! its children's array), dropped inside the DOM change that makes it old.

use super::*;

use rook_dom::Observed;

/// Drop an instance's kept `Name` inside every rename, and its kept
/// children inside every change to them, so no script reads either stale.
/// The DOM tells its observer of both with the journal on or off.
pub(super) fn install(lua: &Lua) {
    let weak = lua.weak();
    with_world(lua, |w| {
        w.dom.set_observer(move |change| {
            // Gone with the VM: nothing is kept.
            let Some(lua) = weak.try_upgrade() else {
                return;
            };
            let c = ctx(&lua);
            let (id, kept) = match change {
                Observed::Renamed(id) => (id, &c.names),
                Observed::ChildrenChanged(id) => (id, &c.kept_children),
            };
            // Only tables are touched, never the world, which is borrowed.
            let ud = c.instances.raw_get::<LuaValue>(key_of(id));
            if let Ok(LuaValue::UserData(ud)) = ud {
                kept.raw_set(ud, LuaValue::Nil)
                    .expect("dropping what an instance's change made old");
            }
        });
    });
}
