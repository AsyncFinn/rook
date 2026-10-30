//! Collecting destroyed instances at each journal trim (rook-dom's
//! `collect.rs` has the rule): a script's handle holds an instance, as do
//! the few things the runtime keeps for later, and what the runtime keeps
//! about a freed instance goes with it.

use mlua::Value as LuaValue;

use crate::Runtime;
use crate::api::instance::key_of;
use crate::ctx::ctx;

impl Runtime {
    /// Drop the journal's entries before `cursor`, which every reader has
    /// read: free the destroyed instances the last trim announced that
    /// nothing holds yet, then announce the ones nothing holds now.
    /// Returns the freed instances.
    pub fn trim_journal(&self, cursor: u64) -> Vec<rook_dom::InstanceId> {
        let c = ctx(&self.lua);
        // A userdata no script can reach leaves the (weak) table when
        // Luau collects it.
        let has_handle = |id| !matches!(c.instances.raw_get(key_of(id)), Ok(LuaValue::Nil));
        let freed = {
            let mut w = self.world_mut();
            let kept = w.held();
            let held = |id| kept.contains(&id) || has_handle(id);
            let freed = w.dom.free_collected(held);
            w.dom.journal_mut().trim(cursor);
            w.dom.announce_collectable(held);
            for &id in &freed {
                w.forget(id);
                w.signals.disconnect_instance(id);
            }
            freed
        };
        let mut runtime_props = c.runtime_prop_ids.borrow_mut();
        let props = c.runtime_props.borrow();
        for &id in &freed {
            if runtime_props.remove(&id)
                && let Some(all) = &*props
                && let Err(e) = all.raw_set(key_of(id), LuaValue::Nil)
            {
                tracing::warn!("dropping a freed instance's values failed: {e}");
            }
            if let Err(e) = crate::api::methods::forget_callbacks(&self.lua, id) {
                tracing::warn!("dropping a freed instance's callbacks failed: {e}");
            }
        }
        freed
    }
}
