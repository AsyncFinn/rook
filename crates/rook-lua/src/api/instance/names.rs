//! The names the `inst.Key` shim keeps, dropped as instances are renamed.

use super::*;

/// Drop an instance's kept `Name` inside every rename (`Dom::set_name` is
/// the only way a name changes), so no script can read the old one.
pub(super) fn install(lua: &Lua) {
    let weak = lua.weak();
    with_world(lua, |w| {
        w.dom.set_rename_observer(move |id| {
            // Gone with the VM: nothing is kept.
            let Some(lua) = weak.try_upgrade() else {
                return;
            };
            let c = ctx(&lua);
            // Only tables are touched, never the world, which is borrowed.
            let ud = c.instances.raw_get::<LuaValue>(key_of(id));
            if let Ok(LuaValue::UserData(ud)) = ud {
                c.names
                    .raw_set(ud, LuaValue::Nil)
                    .expect("dropping a renamed instance's kept Name");
            }
        });
    });
}
