//! UserInputService's gamepad queries and `GetLastInputType`
//! (docs/behavior/input/gamepads.md). The engine keeps the slots' state
//! as pads connect and move ([`crate::gamepads`]).

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;
use rook_types::{GamepadKey, UserInputType};

use super::sequence;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, with_world};
use crate::types::enums::{enum_item, item_value};

/// The slot an argument names: 0 for Gamepad1. None for any other input
/// type, which every query answers as a pad that isn't there.
fn pad_arg(lua: &Lua, a: &Args, i: usize) -> ApiResult<Option<usize>> {
    let value =
        item_value(lua, a.get(i), "UserInputType").ok_or_else(|| a.wrong(i, "UserInputType"))?;
    Ok(UserInputType::from_u32(value).and_then(UserInputType::gamepad_index))
}

fn key_arg(lua: &Lua, a: &Args, i: usize) -> ApiResult<Option<GamepadKey>> {
    let value = item_value(lua, a.get(i), "KeyCode").ok_or_else(|| a.wrong(i, "KeyCode"))?;
    Ok(GamepadKey::from_u32(value))
}

fn items(lua: &Lua, name: &str, values: impl IntoIterator<Item = u32>) -> ApiResult<Outcome> {
    let t = lua.create_table()?;
    for (i, v) in values.into_iter().enumerate() {
        t.raw_set(i + 1, enum_item(lua, name, v)?)?;
    }
    Outcome::ret(lua, t)
}

/// Focus until the first input, as a Studio client starts.
pub(super) fn last_input_type(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let t = with_world(lua, |w| w.input.last_input_type).unwrap_or(UserInputType::Focus);
    Outcome::ret(lua, enum_item(lua, "UserInputType", t.value())?)
}

pub(super) fn connected_gamepads(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let pads = with_world(lua, |w| {
        (w.input.gamepads.iter().zip(UserInputType::GAMEPADS))
            .filter(|(pad, _)| pad.connected)
            .map(|(_, t)| t.value())
            .collect::<Vec<_>>()
    });
    items(lua, "UserInputType", pads)
}

pub(super) fn gamepad_connected(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let pad = pad_arg(lua, a, 0)?;
    let on = pad.is_some_and(|p| with_world(lua, |w| w.input.gamepads[p].connected));
    Outcome::ret(lua, on)
}

/// The slot's 24 InputObjects, the same ones every call and every event
/// passes, whether or not a pad is connected (measured).
pub(super) fn gamepad_state(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let Some(pad) = pad_arg(lua, a, 0)? else {
        return sequence(lua, Vec::new());
    };
    let objects = with_world(lua, |w| {
        let w = &mut *w;
        w.input.gamepads[pad].objects(&mut w.dom, pad)
    });
    sequence(lua, objects)
}

pub(super) fn supported_keys(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let keys = match pad_arg(lua, a, 0)? {
        Some(pad) => with_world(lua, |w| {
            let p = &w.input.gamepads[pad];
            if p.connected {
                p.supported.clone()
            } else {
                Vec::new()
            }
        }),
        None => Vec::new(),
    };
    items(lua, "KeyCode", keys.into_iter().map(GamepadKey::value))
}

pub(super) fn gamepad_supports(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (pad, key) = (pad_arg(lua, a, 0)?, key_arg(lua, a, 1)?);
    let yes = match (pad, key) {
        (Some(pad), Some(key)) => with_world(lua, |w| {
            let p = &w.input.gamepads[pad];
            p.connected && p.supported.contains(&key)
        }),
        _ => false,
    };
    Outcome::ret(lua, yes)
}

pub(super) fn is_gamepad_button_down(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (pad, key) = (pad_arg(lua, a, 0)?, key_arg(lua, a, 1)?);
    let down = match (pad, key) {
        (Some(pad), Some(key)) => with_world(lua, |w| w.input.gamepads[pad].is_down(key)),
        _ => false,
    };
    Outcome::ret(lua, down)
}

/// Every slot, connected or not, until a script takes one off (measured:
/// all eight with no pad connected).
pub(super) fn navigation_gamepads(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let pads = with_world(lua, |w| {
        (w.input.gamepads.iter().zip(UserInputType::GAMEPADS))
            .filter(|(pad, _)| !pad.not_navigation)
            .map(|(_, t)| t.value())
            .collect::<Vec<_>>()
    });
    items(lua, "UserInputType", pads)
}

pub(super) fn is_navigation_gamepad(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let pad = pad_arg(lua, a, 0)?;
    let yes = pad.is_some_and(|p| with_world(lua, |w| !w.input.gamepads[p].not_navigation));
    Outcome::ret(lua, yes)
}

pub(super) fn set_navigation_gamepad(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let pad = pad_arg(lua, a, 0)?;
    let on = match a.get(1) {
        LuaValue::Boolean(b) => *b,
        _ => return Err(a.wrong(1, "boolean")),
    };
    if let Some(pad) = pad {
        with_world(lua, |w| w.input.gamepads[pad].not_navigation = !on);
    }
    Outcome::ret(lua, ())
}
