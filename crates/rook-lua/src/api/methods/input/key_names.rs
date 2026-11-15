//! What UserInputService calls each key: `GetStringForKeyCode` and
//! `GetImageForKeyCode` (docs/behavior/input/gamepads.md#names-and-images).

use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::GamepadKey;

use crate::gamepads::GamepadKind;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, with_world};
use crate::types::enums::item_name;

pub(super) fn string_for_key(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = item_name(lua, a.get(0), "KeyCode").ok_or_else(|| a.wrong(0, "KeyCode"))?;
    let kind = with_world(lua, |w| w.input.key_names);
    let text = match gamepad_key(name) {
        Some(key) => pad_name(kind, key),
        None => typed(name),
    };
    Outcome::ret(lua, text)
}

pub(super) fn image_for_key(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = item_name(lua, a.get(0), "KeyCode").ok_or_else(|| a.wrong(0, "KeyCode"))?;
    let kind = with_world(lua, |w| w.input.key_names);
    let url = gamepad_key(name)
        .map(|key| pad_image(kind, key))
        .unwrap_or_default();
    Outcome::ret(lua, url)
}

fn gamepad_key(name: &str) -> Option<GamepadKey> {
    GamepadKey::ITEMS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|&(_, k)| k)
}

/// What a key types on a US keyboard, measured for every KeyCode in
/// Studio 0.747: its own name for letters and the keys not listed.
fn typed(name: &'static str) -> &'static str {
    match name {
        "Space" => " ",
        "QuotedDouble" | "Quote" => "'",
        "Hash" | "Three" => "3",
        "Dollar" | "Four" => "4",
        "Percent" | "Five" => "5",
        "Ampersand" | "Seven" => "7",
        "LeftParenthesis" | "Nine" => "9",
        "RightParenthesis" | "Zero" => "0",
        "Asterisk" | "Eight" => "8",
        "Plus" | "Equals" => "=",
        "Comma" | "LessThan" => ",",
        "Minus" | "Underscore" | "KeypadMinus" => "-",
        "Period" | "GreaterThan" => ".",
        "Slash" | "Question" => "/",
        "One" => "1",
        "Two" | "At" => "2",
        "Six" | "Caret" => "6",
        "Colon" | "Semicolon" => ";",
        "LeftBracket" | "LeftCurly" => "[",
        "BackSlash" | "Pipe" => "\\",
        "RightBracket" | "RightCurly" => "]",
        "Backquote" | "Tilde" => "`",
        "KeypadPlus" => "+",
        "Print" => "*",
        name if name.starts_with("World") => "",
        "None" | "Backspace" | "Tab" | "Clear" | "Return" | "Pause" | "Escape" | "Delete"
        | "KeypadZero" | "KeypadOne" | "KeypadTwo" | "KeypadThree" | "KeypadFour"
        | "KeypadFive" | "KeypadSix" | "KeypadSeven" | "KeypadEight" | "KeypadNine"
        | "KeypadPeriod" | "KeypadDivide" | "KeypadMultiply" | "KeypadEnter" | "KeypadEquals"
        | "Up" | "Down" | "Right" | "Left" | "Insert" | "Home" | "End" | "PageUp" | "PageDown"
        | "F1" | "F2" | "F3" | "F4" | "F5" | "F6" | "F7" | "F8" | "F9" | "F10" | "F11" | "F12"
        | "F13" | "F14" | "F15" | "NumLock" | "CapsLock" | "ScrollLock" | "RightShift"
        | "LeftShift" | "RightControl" | "LeftControl" | "RightAlt" | "LeftAlt" | "RightMeta"
        | "LeftMeta" | "LeftSuper" | "RightSuper" | "Mode" | "Compose" | "Help" | "SysReq"
        | "Break" | "Menu" | "Power" | "Euro" | "Undo" | "Thumbstick1Up" | "Thumbstick1Down"
        | "Thumbstick1Left" | "Thumbstick1Right" | "Thumbstick2Up" | "Thumbstick2Down"
        | "Thumbstick2Left" | "Thumbstick2Right" | "MouseLeftButton" | "MouseRightButton"
        | "MouseMiddleButton" | "MouseBackButton" | "MouseNoButton" | "MouseX" | "MouseY"
        | "MousePosition" | "TouchPosition" | "MouseWheel" | "TrackpadPan" | "TrackpadPinch"
        | "MouseDelta" | "TouchDelta" | "TouchPinch" => "",
        _ => name,
    }
}

/// A pad key's name on this kind of pad. The keys pads have no name for
/// give "" (measured: ButtonCenter on every kind).
fn pad_name(kind: GamepadKind, key: GamepadKey) -> &'static str {
    use GamepadKey::*;
    match (kind, key) {
        (_, ButtonCenter | ButtonBack | ButtonUp | ButtonDown | ButtonLeft | ButtonRight) => "",
        (GamepadKind::PlayStation4 | GamepadKind::PlayStation5, _) => playstation_name(key),
        (GamepadKind::Xbox, _) => xbox_name(key),
        (GamepadKind::Default, _) => key.name(),
    }
}

/// Measured with a PlayStation 5 pad.
fn playstation_name(key: GamepadKey) -> &'static str {
    use GamepadKey::*;
    match key {
        ButtonA => "ButtonCross",
        ButtonB => "ButtonCircle",
        ButtonX => "ButtonSquare",
        ButtonY => "ButtonTriangle",
        ButtonStart => "ButtonOptions",
        ButtonSelect => "ButtonShare",
        _ => key.name(),
    }
}

/// Measured with an Xbox pad.
fn xbox_name(key: GamepadKey) -> &'static str {
    use GamepadKey::*;
    match key {
        ButtonL1 => "ButtonLB",
        ButtonR1 => "ButtonRB",
        ButtonL2 => "ButtonLT",
        ButtonR2 => "ButtonRT",
        ButtonL3 => "ButtonLS",
        ButtonR3 => "ButtonRS",
        _ => key.name(),
    }
}

/// The install's picture of a pad key: each kind's folder under
/// `textures/ui/Controls`, named as [`pad_name`] names the key, or as the
/// KeyCode for keys with no name. A PlayStation's Options and Share are in
/// the folder of its generation.
fn pad_image(kind: GamepadKind, key: GamepadKey) -> String {
    let named = match pad_name(kind, key) {
        "" => key.name(),
        name => name,
    };
    let folder = match (kind, named) {
        (GamepadKind::Default, _) => "DefaultController",
        (GamepadKind::Xbox, _) => "XboxController",
        (GamepadKind::PlayStation4, "ButtonOptions" | "ButtonShare") => "PlayStationController/PS4",
        (GamepadKind::PlayStation5, "ButtonOptions" | "ButtonShare") => "PlayStationController/PS5",
        (_, _) => "PlayStationController",
    };
    format!("rbxasset://textures/ui/Controls/{folder}/{named}@2x.png")
}
