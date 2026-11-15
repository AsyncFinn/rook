//! Firing signals: each handler now or deferred, and the signals every
//! journal entry fires.

use super::*;

/// Fire a signal with already-converted arguments.
pub(crate) fn fire(lua: &Lua, key: &SignalKey, args: &[LuaValue]) {
    fire_with(lua, key, || args.to_vec());
}

/// Fire a signal, asking `args` for each handler's arguments.
pub(crate) fn fire_with(lua: &Lua, key: &SignalKey, args: impl Fn() -> Vec<LuaValue>) {
    let (handlers, behavior) =
        with_world(lua, |w| (w.signals.take_handlers(key), w.signal_behavior));
    for (connection, handler) in handlers {
        // A stopped script's connections are disconnected.
        if let Handler::Call(f) = &handler
            && stopped_function(lua, f)
        {
            with_world(lua, |w| w.signals.disconnect(connection));
            continue;
        }
        match behavior {
            SignalBehavior::Deferred => with_world(lua, |w| {
                w.scheduler.defer(Job::Fire {
                    connection,
                    handler,
                    args: args(),
                });
            }),
            SignalBehavior::Immediate => {
                let entered = with_world(lua, |w| w.signals.enter());
                if let Err(e) = entered {
                    with_world(lua, |w| w.push_log(MessageType::Error, e.to_string()));
                    continue;
                }
                run_handler(lua, handler, args());
                with_world(lua, |w| w.signals.leave());
            }
        }
    }
}

/// Fire the signals for every journal entry from `start` to the journal's
/// end as of this call. Entries added by handlers are dispatched by the
/// mutation that added them, so Immediate handlers nest the way Roblox's do.
/// Scripts an entry disables or moves out of where they run stop first.
/// A focused TextBox that moves, or whose ancestor does, loses focus here:
/// before the move's signals when it leaves the DataModel, after them when
/// it stays (measured; docs/behavior/gui/input.md#textbox).
pub(crate) fn dispatch(lua: &Lua, start: u64) {
    let end = with_world(lua, |w| w.dom.journal().end());
    rook_types::count!(JournalReadSignals, end.saturating_sub(start));
    let mut destroyed: Vec<InstanceId> = Vec::new();
    for pos in start..end {
        let mut moved_focus = None;
        let mut moved = None;
        let mut stopped = Vec::new();
        let firings = with_world(lua, |w| {
            let change = w.dom.journal().get(pos)?;
            stopped = crate::api::scripts::stopped_by(w, change);
            match change {
                Change::Destroyed(id) => destroyed.push(*id),
                Change::Reparented {
                    id,
                    subtree,
                    new_ancestors,
                    ..
                } => {
                    moved = Some(*id);
                    let left = new_ancestors.last() != Some(&w.dom.root());
                    moved_focus = (w.gui.focus())
                        .filter(|f| subtree.contains(f))
                        .map(|f| (f, left));
                }
                _ => {}
            }
            if w.signals.is_empty() {
                return None;
            }
            Some(crate::signal::expand(change, &w.signals, &w.dom))
        });
        if let Some(id) = moved {
            with_world(lua, |w| crate::api::methods::tools::retire_mice(w, id));
        }
        for script in stopped {
            crate::api::scripts::stop_script(lua, script);
        }
        if let Some((id, true)) = moved_focus {
            release_moved(lua, id);
        }
        for firing in firings.into_iter().flatten() {
            if let Some(args) = firing_args(lua, &firing) {
                fire(lua, &firing.key, &args);
            }
        }
        if let Some((id, false)) = moved_focus {
            release_moved(lua, id);
        }
    }
    if !destroyed.is_empty() {
        with_world(lua, |w| {
            for id in destroyed {
                w.signals.disconnect_instance(id);
            }
        });
    }
}

/// Take focus from TextBox `id`, which moved, as `ReleaseFocus()` does.
fn release_moved(lua: &Lua, id: InstanceId) {
    // An Immediate handler may have moved focus elsewhere already.
    if crate::api::methods::gui::textbox::focused(lua) != Some(id) {
        return;
    }
    if let Err(e) = crate::api::methods::gui::textbox::release(lua, false, LuaValue::Nil, false) {
        with_world(lua, |w| w.push_log(MessageType::Error, e.to_string()));
    }
}

/// Convert a firing's arguments. `None` means the firing doesn't apply
/// (ValueBase `Changed` only fires for `Value`).
fn firing_args(lua: &Lua, firing: &Firing) -> Option<Vec<LuaValue>> {
    let inst = |id| push_instance(lua, id).ok().map(LuaValue::UserData);
    Some(match &firing.args {
        FireArgs::None => Vec::new(),
        FireArgs::Instance(id) => vec![inst(*id)?],
        FireArgs::Ancestry { child, parent } => vec![
            inst(*child)?,
            parent.and_then(inst).unwrap_or(LuaValue::Nil),
        ],
        FireArgs::Attribute(name) => vec![LuaValue::String(lua.create_string(name).ok()?)],
        FireArgs::Health(pid) => {
            // `Health` reads the stored `Health_XML`.
            if with_world(lua, |w| w.dom.database().property(*pid).name) != "Health_XML" {
                return None;
            }
            vec![crate::api::instance::read_property(lua, firing.key.instance, "Health").ok()?]
        }
        FireArgs::Mouse => vec![
            crate::api::methods::tools::equipped_mouse(lua, firing.key.instance)
                .and_then(inst)
                .unwrap_or(LuaValue::Nil),
        ],
        FireArgs::Property(pid) => {
            let id = firing.key.instance;
            let (is_value_base, name) = with_world(lua, |w| {
                let db = w.dom.database();
                let name = crate::api::properties::changed_name(w, id, db.property(*pid).name);
                (w.dom.is_a(id, "ValueBase"), name)
            });
            if is_value_base {
                // Confirmed: creator-docs ValueBase classes: Changed fires
                // with the new value, and only for Value.
                if name != "Value" {
                    return None;
                }
                vec![crate::api::instance::read_property(lua, id, name).ok()?]
            } else {
                vec![LuaValue::String(lua.create_string(name).ok()?)]
            }
        }
    })
}
