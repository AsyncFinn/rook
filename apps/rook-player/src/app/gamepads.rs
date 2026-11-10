//! Gamepads, read through gilrs and given to the game as `InputEvent`s
//! (docs/behavior/input/gamepads.md). gilrs' own filters are off: Studio
//! passes a stick's raw values, jitter included, with no deadzone.

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs, GilrsBuilder};
use rook_engine::{GamepadAxis, GamepadKind, InputEvent};
use rook_types::GamepadKey;

/// The pads the game knows, by slot (0 is Gamepad1).
pub(crate) struct Gamepads {
    /// None where the platform has no gamepad API.
    gilrs: Option<Gilrs>,
    slots: [Option<GamepadId>; 8],
    /// Pads gilrs found when it started, given to the game on the first
    /// read: gilrs announces only pads that come later.
    found: Vec<GamepadId>,
}

impl Gamepads {
    pub(crate) fn new() -> Self {
        let gilrs = GilrsBuilder::new()
            .with_default_filters(false)
            .build()
            .inspect_err(|e| tracing::warn!("gamepads won't be read: {e}"))
            .ok();
        let found = gilrs
            .as_ref()
            .map(|g| g.gamepads().map(|(id, _)| id).collect::<Vec<_>>())
            .unwrap_or_default();
        tracing::info!("gamepads: {} connected at start", found.len());
        Self {
            gilrs,
            slots: [None; 8],
            found,
        }
    }

    /// What the pads did since the last read, in order. An unfocused
    /// window hears nothing from them (measured), but a release still
    /// comes so no key stays held (Inference).
    pub(crate) fn read(&mut self, focused: bool, mut send: impl FnMut(InputEvent)) {
        let Some(gilrs) = &mut self.gilrs else {
            return;
        };
        for id in std::mem::take(&mut self.found) {
            connect(gilrs, &mut self.slots, id, &mut send);
        }
        while let Some(event) = gilrs.next_event() {
            let Some(pad) = self.slots.iter().position(|s| *s == Some(event.id)) else {
                if event.event == EventType::Connected {
                    connect(gilrs, &mut self.slots, event.id, &mut send);
                }
                continue;
            };
            let input = match event.event {
                EventType::Disconnected => {
                    tracing::info!("gamepad {} disconnected", pad + 1);
                    self.slots[pad] = None;
                    Some(InputEvent::GamepadDisconnected { pad })
                }
                EventType::ButtonPressed(b, _) if focused => {
                    button(b).map(|key| InputEvent::GamepadButton {
                        pad,
                        key,
                        down: true,
                    })
                }
                EventType::ButtonReleased(b, _) => button(b).map(|key| InputEvent::GamepadButton {
                    pad,
                    key,
                    down: false,
                }),
                EventType::ButtonChanged(b, value, _) if focused => {
                    trigger(b).map(|axis| InputEvent::GamepadAxis { pad, axis, value })
                }
                EventType::AxisChanged(a, value, _) => match stick(a) {
                    Some(axis) => focused.then_some(InputEvent::GamepadAxis { pad, axis, value }),
                    None => {
                        dpad(a, value, pad, focused, &mut send);
                        None
                    }
                },
                _ => None,
            };
            if let Some(input) = input {
                send(input);
            }
        }
    }
}

/// Give a pad the lowest free slot; Studio doesn't renumber the others
/// when one leaves (measured).
fn connect(
    gilrs: &Gilrs,
    slots: &mut [Option<GamepadId>; 8],
    id: GamepadId,
    send: &mut impl FnMut(InputEvent),
) {
    if slots.contains(&Some(id)) {
        return;
    }
    let Some(pad) = slots.iter().position(Option::is_none) else {
        return;
    };
    let Some(g) = gilrs.connected_gamepad(id) else {
        return;
    };
    slots[pad] = Some(id);
    let kind = kind(g.vendor_id(), g.product_id());
    tracing::info!(
        "gamepad {} connected: {} ({:04x}:{:04x}, {kind:?}, mapped: {})",
        pad + 1,
        g.name(),
        g.vendor_id().unwrap_or(0),
        g.product_id().unwrap_or(0),
        g.mapping_source() != gilrs::MappingSource::None,
    );
    // A stick is listed once per axis, as Studio lists it (measured).
    let mut supported = Vec::new();
    for &(_, key) in GamepadKey::ITEMS {
        let has = match key {
            GamepadKey::Thumbstick1 => {
                [Axis::LeftStickX, Axis::LeftStickY].map(|a| g.axis_code(a).is_some())
            }
            GamepadKey::Thumbstick2 => {
                [Axis::RightStickX, Axis::RightStickY].map(|a| g.axis_code(a).is_some())
            }
            _ => {
                let mapped = gilrs_button(key).is_some_and(|b| g.button_code(b).is_some())
                    || dpad_axis(key).is_some_and(|a| g.axis_code(a).is_some());
                [mapped, false]
            }
        };
        supported.extend(has.into_iter().filter(|&h| h).map(|_| key));
    }
    send(InputEvent::GamepadConnected {
        pad,
        kind,
        supported,
    });
}

/// Which pad it is, which names its keys (docs: key names). By USB ids:
/// Sony's DualShock 4 (two revisions) and DualSense (and its Edge).
fn kind(vendor: Option<u16>, product: Option<u16>) -> GamepadKind {
    match (vendor, product) {
        (Some(0x054c), Some(0x05c4 | 0x09cc)) => GamepadKind::PlayStation4,
        (Some(0x054c), Some(0x0ce6 | 0x0df2)) => GamepadKind::PlayStation5,
        (Some(0x045e), _) => GamepadKind::Xbox,
        _ => GamepadKind::Default,
    }
}

/// The gilrs button for a Roblox key; triggers are axes here and the
/// D-pad may be either.
fn gilrs_button(key: GamepadKey) -> Option<Button> {
    Some(match key {
        GamepadKey::ButtonA => Button::South,
        GamepadKey::ButtonB => Button::East,
        GamepadKey::ButtonX => Button::West,
        GamepadKey::ButtonY => Button::North,
        GamepadKey::ButtonL1 => Button::LeftTrigger,
        GamepadKey::ButtonR1 => Button::RightTrigger,
        GamepadKey::ButtonL2 => Button::LeftTrigger2,
        GamepadKey::ButtonR2 => Button::RightTrigger2,
        GamepadKey::ButtonL3 => Button::LeftThumb,
        GamepadKey::ButtonR3 => Button::RightThumb,
        GamepadKey::ButtonStart => Button::Start,
        GamepadKey::ButtonSelect => Button::Select,
        GamepadKey::DPadUp => Button::DPadUp,
        GamepadKey::DPadDown => Button::DPadDown,
        GamepadKey::DPadLeft => Button::DPadLeft,
        GamepadKey::DPadRight => Button::DPadRight,
        _ => return None,
    })
}

/// The Roblox key for a gilrs button, triggers aside. The pad's own
/// center button (Button::Mode) fires nothing (measured).
fn button(b: Button) -> Option<GamepadKey> {
    GamepadKey::ITEMS
        .iter()
        .map(|&(_, k)| k)
        .find(|&k| !k.is_trigger() && gilrs_button(k) == Some(b))
}

fn trigger(b: Button) -> Option<GamepadAxis> {
    match b {
        Button::LeftTrigger2 => Some(GamepadAxis::Trigger(0)),
        Button::RightTrigger2 => Some(GamepadAxis::Trigger(1)),
        _ => None,
    }
}

/// gilrs' sticks are right and up positive, as Roblox's are.
fn stick(a: Axis) -> Option<GamepadAxis> {
    match a {
        Axis::LeftStickX => Some(GamepadAxis::StickX(0)),
        Axis::LeftStickY => Some(GamepadAxis::StickY(0)),
        Axis::RightStickX => Some(GamepadAxis::StickX(1)),
        Axis::RightStickY => Some(GamepadAxis::StickY(1)),
        _ => None,
    }
}

fn dpad_axis(key: GamepadKey) -> Option<Axis> {
    match key {
        GamepadKey::DPadLeft | GamepadKey::DPadRight => Some(Axis::DPadX),
        GamepadKey::DPadUp | GamepadKey::DPadDown => Some(Axis::DPadY),
        _ => None,
    }
}

/// A pad whose D-pad gilrs maps as two axes: each axis is two buttons.
fn dpad(a: Axis, value: f32, pad: usize, focused: bool, send: &mut impl FnMut(InputEvent)) {
    let (less, more) = match a {
        Axis::DPadX => (GamepadKey::DPadLeft, GamepadKey::DPadRight),
        Axis::DPadY => (GamepadKey::DPadDown, GamepadKey::DPadUp),
        _ => return,
    };
    for (key, down) in [(less, value < -0.5), (more, value > 0.5)] {
        if focused || !down {
            send(InputEvent::GamepadButton { pad, key, down });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every button and trigger Studio names maps both ways, and the
    /// center button maps to nothing.
    #[test]
    fn keys_map_both_ways() {
        for &(_, key) in GamepadKey::ITEMS {
            let Some(b) = gilrs_button(key) else {
                continue;
            };
            if key.is_trigger() {
                assert!(trigger(b).is_some(), "{key:?}");
            } else {
                assert_eq!(button(b), Some(key));
            }
        }
        assert_eq!(button(Button::Mode), None);
        assert_eq!(kind(Some(0x054c), Some(0x0ce6)), GamepadKind::PlayStation5);
    }
}
