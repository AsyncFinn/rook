//! The names the `inst.Key` shim keeps, and dropping them on renames.

use super::*;

use rook_dom::Change;

/// Drop the kept `Name` of every instance renamed since the last call,
/// read from the journal. Called wherever control passes back to Luau, so
/// the next read by any script sees a rename.
pub(crate) fn note_renames(c: &Ctx) -> mlua::Result<()> {
    let renamed: Vec<InstanceId> = {
        let w = c.world.borrow();
        let journal = w.dom.journal();
        let cursor = c.names_cursor.replace(journal.end());
        if cursor == journal.end() {
            return Ok(());
        }
        // Trimmed before it was read: any name may be stale.
        if cursor < journal.start() {
            drop(w);
            return c.names.clear();
        }
        let name = w.dom.name_property();
        (journal.since(cursor).iter())
            .filter_map(|change| match change {
                Change::Property { id, property } if *property == name => Some(*id),
                _ => None,
            })
            .collect()
    };
    for id in renamed {
        if let LuaValue::UserData(ud) = c.instances.raw_get::<LuaValue>(key_of(id))? {
            c.names.raw_set(ud, LuaValue::Nil)?;
        }
    }
    Ok(())
}
