//! UserGameSettings' members, and the GuiService and SoundService ones
//! that change the player's settings (docs/behavior/user-settings.md).

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;
use rook_types::Value;

use super::{RawMethod, mutate};
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};
use crate::signal::SignalKey;
use crate::threads::fire;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("UserGameSettings", "InFullScreen") => in_full_screen,
        ("UserGameSettings", "GetCameraYInvertValue") => camera_y_invert_value,
        ("UserGameSettings", "SetCameraYInvertVisible") => camera_y_invert_visible,
        ("UserGameSettings", "SetGamepadCameraSensitivityVisible") => gamepad_sensitivity_visible,
        ("GuiService", "ToggleFullscreen") => toggle_fullscreen,
        ("SoundService", "GetOutputDevices") => get_output_devices,
        ("SoundService", "SetOutputDevice") => set_output_device,
        _ => return None,
    })
}

fn flag(lua: &Lua, this: InstanceId, name: &str) -> bool {
    with_world(lua, |w| w.dom.bool_property(this, name)) == Some(true)
}

/// Approximation: Roblox reports the window; Rook's player keeps its
/// window as `Fullscreen` says, so the setting is the answer.
fn in_full_screen(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, flag(lua, this, "Fullscreen"))
}

/// What the camera scripts multiply vertical turning by: -1 when the
/// player inverted it (creator-docs UserGameSettings:GetCameraYInvertValue).
fn camera_y_invert_value(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let value = if flag(lua, this, "CameraYInverted") {
        -1
    } else {
        1
    };
    Outcome::ret(lua, value)
}

/// The camera scripts say they follow `CameraYInverted`, so the menu
/// offers it (creator-docs UserGameSettings:SetCameraYInvertVisible).
fn camera_y_invert_visible(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    mutate(lua, |w| {
        w.dom
            .set_property_by_name(this, "IsUsingCameraYInverted", Value::Bool(true))
    })?;
    Outcome::ret(lua, ())
}

/// The camera scripts say they follow `GamepadCameraSensitivity`, so the
/// menu offers it (creator-docs
/// UserGameSettings:SetGamepadCameraSensitivityVisible).
fn gamepad_sensitivity_visible(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    mutate(lua, |w| {
        w.dom
            .set_property_by_name(this, "IsUsingGamepadCameraSensitivity", Value::Bool(true))
    })?;
    Outcome::ret(lua, ())
}

/// The settings menu's fullscreen switch: the setting flips, which the
/// player's window follows, and `FullscreenChanged` says so.
fn toggle_fullscreen(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let (game, on) = mutate(lua, |w| {
        let (_, game) = crate::user_settings::ensure(w);
        let on = w.dom.bool_property(game, "Fullscreen") != Some(true);
        w.dom
            .set_property_by_name(game, "Fullscreen", Value::Bool(on))?;
        Ok((game, on))
    })?;
    fire(
        lua,
        &SignalKey::event(game, "FullscreenChanged"),
        &[LuaValue::Boolean(on)],
    );
    Outcome::ret(lua, ())
}

/// The settings menu's Output Device list: names, ids and the chosen
/// one's index, 1-based. Measured in Studio: first the system's default,
/// named as that device, then every device by the system's name for it.
/// The default's id is "" (Roblox's own script names it "Default (...)"
/// for that); Rook gives the others their name as id (Inference: Roblox's
/// ids are the system's, which only come back to `SetOutputDevice`).
fn get_output_devices(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let (devices, chosen) = with_world(lua, |w| {
        let chosen = w
            .user_settings
            .and_then(|root| crate::user_settings::game_settings(&w.dom, root))
            .and_then(|game| crate::user_settings::output_device(&w.dom, game));
        (w.output_devices.clone(), chosen)
    });
    let Some(default) = devices.default else {
        // No device: an empty list, which the menu shows as no choices.
        return Outcome::ret(lua, (lua.create_table()?, lua.create_table()?, 0));
    };
    let mut names = vec![default];
    let mut ids = vec![String::new()];
    names.extend(devices.names.iter().cloned());
    ids.extend(devices.names);
    let index = chosen
        .and_then(|c| ids.iter().position(|id| *id == c))
        .unwrap_or(0);
    Outcome::ret(
        lua,
        (
            lua.create_sequence_from(names)?,
            lua.create_sequence_from(ids)?,
            index + 1,
        ),
    )
}

/// Play through the device with this id from now on, and keep the choice
/// (`RookOutputDevice`); "" is the system's default.
fn set_output_device(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let id = a.string(1)?;
    let device = (!id.is_empty()).then_some(id);
    with_world(lua, |w| crate::user_settings::set_output_device(w, device));
    Outcome::ret(lua, ())
}
