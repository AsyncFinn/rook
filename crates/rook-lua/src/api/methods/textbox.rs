//! TextBox focus: `CaptureFocus`, `ReleaseFocus`, `IsFocused` and
//! `UserInputService:GetFocusedTextBox`, and the same focus moves the
//! engine makes for clicks and keys (docs/behavior/gui/input.md#textbox).

use mlua::{IntoLua, Lua, Value as LuaValue};
use rook_dom::InstanceId;
use rook_types::Value;

use super::RawMethod;
use crate::api::instance::push_instance;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};
use crate::signal::SignalKey;
use crate::threads::{dispatch, fire};

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("TextBox", "CaptureFocus") => capture_focus,
        ("TextBox", "ReleaseFocus") => release_focus,
        ("TextBox", "IsFocused") => is_focused,
        ("UserInputService", "GetFocusedTextBox") => get_focused_text_box,
        _ => return None,
    })
}

/// Measured: takes effect at once (`Text` empty and `IsFocused` true when
/// it returns), the caret at the end without `ClearTextOnFocus`.
fn capture_focus(lua: &Lua, id: InstanceId, _: &Args) -> ApiResult<Outcome> {
    focus(lua, id, None)?;
    Outcome::ret(lua, ())
}

/// Measured: `FocusLost` gets `submitted` and no InputObject.
fn release_focus(lua: &Lua, id: InstanceId, a: &Args) -> ApiResult<Outcome> {
    if focused(lua) == Some(id) {
        release(lua, a.truthy(0), LuaValue::Nil, false)?;
    }
    Outcome::ret(lua, ())
}

fn is_focused(lua: &Lua, id: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, focused(lua) == Some(id))
}

fn get_focused_text_box(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    match focused(lua) {
        Some(id) => Outcome::ret(lua, push_instance(lua, id)?),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}

/// The TextBox with focus, if it's still there.
pub(crate) fn focused(lua: &Lua) -> Option<InstanceId> {
    with_world(lua, |w| {
        let id = w.gui.focus()?;
        w.dom.get(id).is_some().then_some(id)
    })
}

fn fire_service(lua: &Lua, event: &'static str, args: &[LuaValue]) {
    let uis = with_world(lua, |w| w.dom.find_service("UserInputService"));
    if let Some(uis) = uis {
        fire(lua, &SignalKey::event(uis, event), args);
    }
}

/// Give `id` focus, its caret before character `caret` (`None`: at the
/// end). Measured order: `Text` empties (with `ClearTextOnFocus`),
/// `CursorPosition` moves, then `Focused` and UserInputService's
/// `TextBoxFocused`. A box losing focus to it gets `FocusLost(false,
/// nil)` and no `TextBoxFocusReleased`.
pub(crate) fn focus(lua: &Lua, id: InstanceId, caret: Option<usize>) -> mlua::Result<()> {
    let current = focused(lua);
    if current == Some(id) {
        if let Some(caret) = caret {
            let start = with_world(lua, |w| {
                let start = w.dom.journal().end();
                let _ = w
                    .dom
                    .set_property_by_name(id, "SelectionStart", Value::Int32(-1));
                let _ = w
                    .dom
                    .set_property_by_name(id, "CursorPosition", position(caret));
                start
            });
            dispatch(lua, start);
        }
        return Ok(());
    }
    if current.is_some() {
        release(lua, false, LuaValue::Nil, true)?;
    }
    let start = with_world(lua, |w| {
        let start = w.dom.journal().end();
        let clear = w.dom.bool_property(id, "ClearTextOnFocus") != Some(false);
        if clear {
            let _ = w
                .dom
                .set_property_by_name(id, "Text", Value::String(String::new()));
        }
        let length = w
            .dom
            .string_property(id, "Text")
            .unwrap_or_default()
            .chars()
            .count();
        let caret = if clear {
            0
        } else {
            caret.unwrap_or(length).min(length)
        };
        let _ = w
            .dom
            .set_property_by_name(id, "CursorPosition", position(caret));
        let _ = w
            .dom
            .set_property_by_name(id, "SelectionStart", Value::Int32(-1));
        w.gui.set_focus(Some(id));
        start
    });
    dispatch(lua, start);
    fire(lua, &SignalKey::event(id, "Focused"), &[]);
    fire_service(
        lua,
        "TextBoxFocused",
        &[push_instance(lua, id)?.into_lua(lua)?],
    );
    Ok(())
}

/// Take focus from the focused TextBox. Measured order: `CursorPosition`
/// becomes −1, then `FocusLost(enter_pressed, cause)`, then
/// UserInputService's `TextBoxFocusReleased`. A box another takes focus
/// from (`stolen`) keeps its `CursorPosition` and fires only `FocusLost`.
pub(crate) fn release(
    lua: &Lua,
    enter_pressed: bool,
    cause: LuaValue,
    stolen: bool,
) -> mlua::Result<()> {
    let Some(id) = focused(lua) else {
        with_world(lua, |w| w.gui.set_focus(None));
        return Ok(());
    };
    let start = with_world(lua, |w| {
        let start = w.dom.journal().end();
        if !stolen {
            let _ = w
                .dom
                .set_property_by_name(id, "CursorPosition", Value::Int32(-1));
            let _ = w
                .dom
                .set_property_by_name(id, "SelectionStart", Value::Int32(-1));
        }
        w.gui.set_focus(None);
        start
    });
    dispatch(lua, start);
    fire(
        lua,
        &SignalKey::event(id, "FocusLost"),
        &[LuaValue::Boolean(enter_pressed), cause],
    );
    if !stolen {
        fire_service(
            lua,
            "TextBoxFocusReleased",
            &[push_instance(lua, id)?.into_lua(lua)?],
        );
    }
    Ok(())
}

/// `CursorPosition` for the caret before character `index`: 1-based.
fn position(index: usize) -> Value {
    Value::Int32(i32::try_from(index + 1).unwrap_or(i32::MAX))
}
