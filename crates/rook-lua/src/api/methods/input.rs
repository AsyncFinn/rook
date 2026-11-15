//! UserInputService and VRService queries, and Player:GetMouse. Input events themselves
//! are fired by the engine, which owns the window.

mod gamepads;
mod key_names;
mod keyboard_mouse;
mod vr;

use mlua::Lua;
use rook_dom::InstanceId;

use super::RawMethod;
use crate::api::instance::push_instance;
use crate::ctx::{ApiResult, Outcome};

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    use gamepads::*;
    use key_names::*;
    use keyboard_mouse::*;
    use vr::*;
    Some(match (owner, name) {
        ("UserInputService", "IsKeyDown") => is_key_down,
        ("UserInputService", "IsMouseButtonPressed") => is_mouse_button_pressed,
        ("UserInputService", "GetMouseLocation") => get_mouse_location,
        ("UserInputService", "GetMouseDelta") => get_mouse_delta,
        ("UserInputService", "GetKeysPressed") => get_keys_pressed,
        ("UserInputService", "GetMouseButtonsPressed") => get_mouse_buttons_pressed,
        ("UserInputService", "GetLastInputType") => last_input_type,
        ("UserInputService", "GetPlatform") => platform,
        ("UserInputService", "GetConnectedGamepads") => connected_gamepads,
        ("UserInputService", "GetGamepadConnected") => gamepad_connected,
        ("UserInputService", "GetGamepadState") => gamepad_state,
        ("UserInputService", "GetSupportedGamepadKeyCodes") => supported_keys,
        ("UserInputService", "GamepadSupports") => gamepad_supports,
        ("UserInputService", "IsGamepadButtonDown") => is_gamepad_button_down,
        ("UserInputService", "GetNavigationGamepads") => navigation_gamepads,
        ("UserInputService", "IsNavigationGamepad") => is_navigation_gamepad,
        ("UserInputService", "SetNavigationGamepad") => set_navigation_gamepad,
        ("UserInputService", "GetStringForKeyCode") => string_for_key,
        ("UserInputService", "GetImageForKeyCode") => image_for_key,
        ("Player", "GetMouse") => get_mouse,
        ("VRService", "GetUserCFrame") => user_cframe,
        ("VRService", "GetUserCFrameEnabled") => user_cframe_enabled,
        _ => return None,
    })
}

/// A Luau array of these instances.
fn sequence(lua: &Lua, ids: Vec<InstanceId>) -> ApiResult<Outcome> {
    let t = lua.create_table()?;
    for (i, id) in ids.into_iter().enumerate() {
        t.raw_set(i + 1, push_instance(lua, id)?)?;
    }
    Outcome::ret(lua, t)
}

/// Rook runs on desktops; scripts tell them apart only for platform
/// features Rook doesn't have.
fn platform(lua: &Lua, _: InstanceId, _: &crate::ctx::args::Args) -> ApiResult<Outcome> {
    let name = if cfg!(target_os = "macos") {
        "OSX"
    } else {
        "Windows"
    };
    Outcome::ret(lua, crate::types::enums::named_item(lua, "Platform", name)?)
}
