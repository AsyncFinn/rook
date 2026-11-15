//! GuiService's `SelectedObject` and `SelectedCoreObject`: moving either
//! fires the GUI selection events, in the order Studio does
//! (docs/behavior/gui/selection.md). Roblox's top bar leaves its focus when
//! `SelectionChanged` says the selection left it.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;
use rook_types::Value;

use crate::api::instance::push_instance;
use crate::runtime::ctx::with_world;
use crate::runtime::signal::SignalKey;
use crate::runtime::threads::fire_with;
use crate::runtime::world::World;

/// A selection about to move from `old` to `new`.
pub(crate) struct Move {
    old: Option<InstanceId>,
    new: Option<InstanceId>,
}

/// Before `value` is stored as `id`'s `name`: when that moves a selection,
/// `SelectionLost` on the old object and `SelectionGained` on the new one,
/// which come before the property's change (measured). Setting the
/// selection to what it is fires nothing.
// TODO: Studio refuses some objects ("Setting GuiService.SelectedObject to
// invalid GuiObject"); which ones is M19 step 3's to measure.
pub(crate) fn before(lua: &Lua, id: InstanceId, name: &str, value: &Value) -> Option<Move> {
    let Value::Ref(r) = value else {
        return None;
    };
    let new = r.map(InstanceId::from_ref);
    let old = with_world(lua, |w| {
        let selection = matches!(name, "SelectedObject" | "SelectedCoreObject")
            && w.dom.class_name(id) == "GuiService";
        selection.then(|| w.dom.ref_property(id, name))
    })?;
    if old == new {
        return None;
    }
    if let Some(old) = old {
        fire_on(lua, old, "SelectionLost", Vec::new);
    }
    if let Some(new) = new {
        fire_on(lua, new, "SelectionGained", Vec::new);
    }
    Some(Move { old, new })
}

impl Move {
    /// After the property's change: `SelectionChanged(amISelected,
    /// previous, new)` on the old object and its GUI ancestors, nearest
    /// first, then on the new one and those of its ancestors not reached
    /// yet. Only the new object hears true (measured).
    pub(crate) fn after(self, lua: &Lua) {
        let order = with_world(lua, |w| {
            let mut order = Vec::new();
            for end in [self.old, self.new].into_iter().flatten() {
                for id in gui_chain(w, end) {
                    if !order.contains(&id) {
                        order.push(id);
                    }
                }
            }
            order
        });
        let object = |id: Option<InstanceId>| {
            id.and_then(|id| push_instance(lua, id).ok())
                .map_or(LuaValue::Nil, LuaValue::UserData)
        };
        for id in order {
            let selected = Some(id) == self.new;
            fire_on(lua, id, "SelectionChanged", || {
                vec![
                    LuaValue::Boolean(selected),
                    object(self.old),
                    object(self.new),
                ]
            });
        }
    }
}

/// `id` and its ancestors that are GUI (GuiBase2d), nearest first.
fn gui_chain(w: &World, id: InstanceId) -> Vec<InstanceId> {
    std::iter::successors(Some(id), |&i| w.dom.parent(i))
        .filter(|&i| w.dom.is_a(i, "GuiBase2d"))
        .collect()
}

fn fire_on(lua: &Lua, id: InstanceId, event: &'static str, args: impl Fn() -> Vec<LuaValue>) {
    let exists = with_world(lua, |w| w.dom.get(id).is_some());
    if exists {
        fire_with(lua, &SignalKey::event(id, event), args);
    }
}
