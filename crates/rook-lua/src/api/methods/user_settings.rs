//! UserGameSettings' members, and the GuiService ones that change the
//! player's settings (docs/behavior/user-settings.md).

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
        ("GuiService", "ToggleFullscreen") => toggle_fullscreen,
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
