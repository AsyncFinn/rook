//! UserInputService queries and Player:GetMouse. Input events themselves
//! are fired by the engine, which owns the window.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;
use rook_types::{UserInputState, UserInputType};

use super::instance::push_instance;
use super::methods::RawMethod;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};
use crate::types::LVector2;
use crate::types::enums::item_value;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("UserInputService", "IsKeyDown") => is_key_down,
        ("UserInputService", "IsMouseButtonPressed") => is_mouse_button_pressed,
        ("UserInputService", "GetMouseLocation") => get_mouse_location,
        ("UserInputService", "GetMouseDelta") => get_mouse_delta,
        ("UserInputService", "GetKeysPressed") => get_keys_pressed,
        ("UserInputService", "GetMouseButtonsPressed") => get_mouse_buttons_pressed,
        ("UserInputService", "GetFocusedTextBox") => nil,
        ("UserInputService", "GetLastInputType") => last_input_type,
        ("Player", "GetMouse") => get_mouse,
        _ => return super::physics::native(owner, name),
    })
}

fn nil(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, LuaValue::Nil)
}

fn is_key_down(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let code = item_value(lua, a.get(0), "KeyCode").ok_or_else(|| a.wrong(0, "KeyCode"))?;
    Outcome::ret(lua, with_world(lua, |w| w.input.keys.contains(&code)))
}

/// `Enum.UserInputType` MouseButton1-3 are values 0-2.
fn is_mouse_button_pressed(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let t =
        item_value(lua, a.get(0), "UserInputType").ok_or_else(|| a.wrong(0, "UserInputType"))?;
    let pressed = with_world(lua, |w| {
        w.input.buttons.get(t as usize).copied().unwrap_or(false)
    });
    Outcome::ret(lua, pressed)
}

fn get_mouse_location(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let (x, y) = with_world(lua, |w| w.input.mouse);
    Outcome::ret(lua, LVector2(rook_types::Vector2::new(x, y)))
}

fn get_mouse_delta(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let (x, y) = with_world(lua, |w| w.input.mouse_delta);
    Outcome::ret(lua, LVector2(rook_types::Vector2::new(x, y)))
}

/// Fresh InputObjects describing what is held (creator-docs
/// UserInputService:GetKeysPressed).
fn get_keys_pressed(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let objects = with_world(lua, |w| {
        let mut keys: Vec<u32> = w.input.keys.iter().copied().collect();
        keys.sort_unstable();
        keys.into_iter()
            .filter_map(|k| {
                crate::input_object(
                    &mut w.dom,
                    k,
                    UserInputType::Keyboard,
                    UserInputState::Begin,
                    (0.0, 0.0),
                    (0.0, 0.0),
                )
            })
            .collect::<Vec<_>>()
    });
    sequence(lua, objects)
}

fn get_mouse_buttons_pressed(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let objects = with_world(lua, |w| {
        let buttons = w.input.buttons;
        let pos = w.input.mouse;
        UserInputType::MOUSE_BUTTONS
            .into_iter()
            .zip(buttons)
            .filter(|&(_, down)| down)
            .filter_map(|(b, _)| {
                crate::input_object(&mut w.dom, 0, b, UserInputState::Begin, pos, (0.0, 0.0))
            })
            .collect::<Vec<_>>()
    });
    sequence(lua, objects)
}

fn sequence(lua: &Lua, ids: Vec<InstanceId>) -> ApiResult<Outcome> {
    let t = lua.create_table()?;
    for (i, id) in ids.into_iter().enumerate() {
        t.raw_set(i + 1, push_instance(lua, id)?)?;
    }
    Outcome::ret(lua, t)
}

/// Keyboard and mouse is the only input Rook has so far.
fn last_input_type(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    // MouseMovement.
    Outcome::ret(
        lua,
        crate::types::enums::enum_item(lua, "UserInputType", UserInputType::Keyboard.value())?,
    )
}

/// The local player's Mouse on a client; nil on the server.
fn get_mouse(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let mouse = with_world(lua, |w| {
        let local = w.dom.local_player()?;
        (local == this).then_some(w.input.mouse_object).flatten()
    });
    match mouse {
        Some(m) => Outcome::ret(lua, push_instance(lua, m)?),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}
