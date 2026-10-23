//! PlayerScripts' movement modes: which camera and movement modes the
//! settings menu offers, registered by the PlayerModule (creator-docs
//! PlayerScripts). Each kind keeps its own list, in the order registered.

use mlua::Lua;
use rook_dom::InstanceId;

use super::RawMethod;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};
use crate::signal::SignalKey;
use crate::threads::fire;
use crate::types::enums::{enum_item, item_value};

/// The four kinds: the enum each holds, and the event registering fires.
const KINDS: [(&str, &str); 4] = [
    (
        "ComputerCameraMovementMode",
        "ComputerCameraMovementModeRegistered",
    ),
    ("ComputerMovementMode", "ComputerMovementModeRegistered"),
    (
        "TouchCameraMovementMode",
        "TouchCameraMovementModeRegistered",
    ),
    ("TouchMovementMode", "TouchMovementModeRegistered"),
];

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    if owner != "PlayerScripts" {
        return None;
    }
    Some(match name {
        "RegisterComputerCameraMovementMode" => |l, t, a| register(l, t, a, 0),
        "RegisterComputerMovementMode" => |l, t, a| register(l, t, a, 1),
        "RegisterTouchCameraMovementMode" => |l, t, a| register(l, t, a, 2),
        "RegisterTouchMovementMode" => |l, t, a| register(l, t, a, 3),
        "ClearComputerCameraMovementModes" => |l, t, _| clear(l, t, 0),
        "ClearComputerMovementModes" => |l, t, _| clear(l, t, 1),
        "ClearTouchCameraMovementModes" => |l, t, _| clear(l, t, 2),
        "ClearTouchMovementModes" => |l, t, _| clear(l, t, 3),
        "GetRegisteredComputerCameraMovementModes" => |l, t, _| registered(l, t, 0),
        "GetRegisteredComputerMovementModes" => |l, t, _| registered(l, t, 1),
        "GetRegisteredTouchCameraMovementModes" => |l, t, _| registered(l, t, 2),
        "GetRegisteredTouchMovementModes" => |l, t, _| registered(l, t, 3),
        _ => return None,
    })
}

fn register(lua: &Lua, this: InstanceId, a: &Args, kind: usize) -> ApiResult<Outcome> {
    let (enum_name, event) = KINDS[kind];
    let value = item_value(lua, a.get(0), enum_name).ok_or_else(|| a.wrong(0, enum_name))?;
    let added = with_world(lua, |w| {
        let list = w.movement_modes.entry((this, kind)).or_default();
        let new = !list.contains(&value);
        if new {
            list.push(value);
        }
        new
    });
    if added {
        fire(lua, &SignalKey::event(this, event), &[]);
    }
    Outcome::ret(lua, ())
}

fn clear(lua: &Lua, this: InstanceId, kind: usize) -> ApiResult<Outcome> {
    with_world(lua, |w| w.movement_modes.remove(&(this, kind)));
    fire(lua, &SignalKey::event(this, KINDS[kind].1), &[]);
    Outcome::ret(lua, ())
}

fn registered(lua: &Lua, this: InstanceId, kind: usize) -> ApiResult<Outcome> {
    let values = with_world(lua, |w| {
        w.movement_modes
            .get(&(this, kind))
            .cloned()
            .unwrap_or_default()
    });
    let list = lua.create_table()?;
    for (i, v) in values.into_iter().enumerate() {
        list.raw_set(i + 1, enum_item(lua, KINDS[kind].0, v)?)?;
    }
    Outcome::ret(lua, list)
}
