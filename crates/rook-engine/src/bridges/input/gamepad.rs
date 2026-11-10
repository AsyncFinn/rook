//! Gamepads as UserInputService hears them: connecting and
//! disconnecting, buttons, thumbsticks and triggers, each through the
//! slot's own InputObject for the key (docs/behavior/input/gamepads.md).

use rook_lua::gamepads::GamepadKind;
use rook_types::{GamepadKey, UserInputState, UserInputType, Value, Vector3};

use super::input_type::Preferred;
use crate::DataModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamepadAxis {
    /// Thumbstick 1's or 2's (0 or 1) X or Y, right and up positive.
    StickX(usize),
    StickY(usize),
    /// ButtonL2's (0) or ButtonR2's (1) pull, 0 to 1.
    Trigger(usize),
}

impl GamepadAxis {
    fn key(self) -> GamepadKey {
        match self {
            Self::StickX(0) | Self::StickY(0) => GamepadKey::Thumbstick1,
            Self::StickX(_) | Self::StickY(_) => GamepadKey::Thumbstick2,
            Self::Trigger(0) => GamepadKey::ButtonL2,
            Self::Trigger(_) => GamepadKey::ButtonR2,
        }
    }
}

/// How far a thumbstick or trigger must be from rest for its change to
/// count as the pad being in use (measured for thumbsticks: 0.2000
/// counted, 0.1843 didn't; Inference that triggers use the same).
const IN_USE: f32 = 0.2;

impl DataModel {
    /// A pad took slot `pad`: `GamepadEnabled`, then `PreferredInput`,
    /// then `GamepadConnected` (measured order). Keys are named after the
    /// pad last connected, however the others are used (measured).
    pub(super) fn gamepad_connected(
        &self,
        pad: usize,
        kind: GamepadKind,
        supported: Vec<GamepadKey>,
    ) -> bool {
        let Some(t) = UserInputType::GAMEPADS.get(pad).copied() else {
            return false;
        };
        let was_enabled = {
            let mut w = self.runtime.world_mut();
            let was = w.input.any_gamepad();
            let slot = &mut w.input.gamepads[pad];
            slot.connected = true;
            slot.kind = kind;
            slot.supported = supported;
            w.input.key_names = kind;
            was
        };
        if !was_enabled {
            self.set_uis_bool("GamepadEnabled", true);
        }
        self.set_preferred_input(Preferred::Gamepad);
        self.fire_gamepad_signal("GamepadConnected", t);
        false
    }

    /// A pad left slot `pad`: its held keys and off-center sticks end
    /// first (measured for sticks; Inference for keys and triggers), then
    /// `GamepadEnabled` and `PreferredInput` follow if it was the last,
    /// and `GamepadDisconnected` fires. The slot keeps its objects.
    pub(super) fn gamepad_disconnected(&self, pad: usize) -> bool {
        let Some(t) = UserInputType::GAMEPADS.get(pad).copied() else {
            return false;
        };
        let (sticks, triggers, down) = {
            let w = self.runtime.world();
            let slot = &w.input.gamepads[pad];
            if !slot.connected {
                return false;
            }
            (slot.sticks, slot.triggers, slot.down)
        };
        for (i, (x, y)) in sticks.into_iter().enumerate() {
            if (x, y) != (0.0, 0.0) {
                self.note_input_type(t, None);
                let key = GamepadAxis::StickX(i).key();
                self.runtime.world_mut().input.gamepads[pad].sticks[i] = (0.0, 0.0);
                self.end_pad_key(pad, key);
            }
        }
        for (i, pull) in triggers.into_iter().enumerate() {
            if pull != 0.0 {
                self.runtime.world_mut().input.gamepads[pad].triggers[i] = 0.0;
                self.end_pad_key(pad, GamepadAxis::Trigger(i).key());
            }
        }
        for &(_, key) in GamepadKey::ITEMS {
            if !key.is_trigger() && down & (1 << key.index()) != 0 {
                self.end_pad_key(pad, key);
            }
        }
        let any_left = {
            let mut w = self.runtime.world_mut();
            let slot = &mut w.input.gamepads[pad];
            slot.connected = false;
            slot.down = 0;
            w.input.any_gamepad()
        };
        if !any_left {
            self.set_uis_bool("GamepadEnabled", false);
            self.set_preferred_input(Preferred::KeyboardAndMouse);
        }
        self.fire_gamepad_signal("GamepadDisconnected", t);
        false
    }

    /// A button down or up: Position.Z 1 or 0, Delta.Z the step
    /// (measured).
    pub(super) fn gamepad_button(&self, pad: usize, key: GamepadKey, down: bool) -> bool {
        let Some(t) = UserInputType::GAMEPADS.get(pad).copied() else {
            return false;
        };
        {
            let mut w = self.runtime.world_mut();
            let slot = &mut w.input.gamepads[pad];
            if !slot.connected || slot.is_down(key) == down {
                return false;
            }
            slot.set_down(key, down);
        }
        self.note_input_type(t, Some(Preferred::Gamepad));
        let (z, dz, state, event) = if down {
            (1.0, 1.0, UserInputState::Begin, "InputBegan")
        } else {
            (0.0, -1.0, UserInputState::End, "InputEnded")
        };
        let Some(obj) = self.pad_object(pad, key) else {
            return false;
        };
        self.update_input_object(
            obj,
            state,
            Vector3::new(0.0, 0.0, z),
            Vector3::new(0.0, 0.0, dz),
        );
        self.deliver(event, obj, false)
    }

    /// An axis moved. A thumbstick fires InputChanged for the axis, with
    /// both axes in Position and only this one in Delta. A trigger fires
    /// InputChanged with its pull in Position.Z, then InputBegan once it
    /// reaches 1 and InputEnded each time it's back at 0, with Delta zero,
    /// even if it never began (measured).
    pub(super) fn gamepad_axis(&self, pad: usize, axis: GamepadAxis, value: f32) -> bool {
        let Some(t) = UserInputType::GAMEPADS.get(pad).copied() else {
            return false;
        };
        let key = axis.key();
        let value = value.clamp(-1.0, 1.0);
        let (position, delta, in_use) = {
            let mut w = self.runtime.world_mut();
            let slot = &mut w.input.gamepads[pad];
            if !slot.connected {
                return false;
            }
            match axis {
                GamepadAxis::StickX(i) | GamepadAxis::StickY(i) => {
                    let (x, y) = slot.sticks[i];
                    let (nx, ny) = match axis {
                        GamepadAxis::StickX(_) => (value, y),
                        _ => (x, value),
                    };
                    if (nx, ny) == (x, y) {
                        return false;
                    }
                    slot.sticks[i] = (nx, ny);
                    let in_use = nx.hypot(ny) >= IN_USE;
                    let position = Vector3::new(nx, ny, 0.0);
                    (position, Vector3::new(nx - x, ny - y, 0.0), in_use)
                }
                GamepadAxis::Trigger(i) => {
                    let value = value.max(0.0);
                    let was = slot.triggers[i];
                    if value == was {
                        return false;
                    }
                    slot.triggers[i] = value;
                    (
                        Vector3::new(0.0, 0.0, value),
                        Vector3::new(0.0, 0.0, value - was),
                        value >= IN_USE,
                    )
                }
            }
        };
        if in_use {
            self.note_input_type(t, Some(Preferred::Gamepad));
        }
        let Some(obj) = self.pad_object(pad, key) else {
            return false;
        };
        self.update_input_object(obj, UserInputState::Change, position, delta);
        let mut processed = self.deliver("InputChanged", obj, false);
        if let GamepadAxis::Trigger(_) = axis {
            let pull = position.z();
            let began = self.runtime.world().input.gamepads[pad].is_down(key);
            let edge = if pull >= 1.0 && !began {
                Some((true, UserInputState::Begin, "InputBegan"))
            } else if pull == 0.0 {
                Some((false, UserInputState::End, "InputEnded"))
            } else {
                None
            };
            if let Some((down, state, event)) = edge {
                self.runtime.world_mut().input.gamepads[pad].set_down(key, down);
                self.update_input_object(obj, state, position, Vector3::new(0.0, 0.0, 0.0));
                processed = self.deliver(event, obj, false);
            }
        }
        processed
    }

    /// End a key while its pad disconnects: at rest, Delta zero.
    fn end_pad_key(&self, pad: usize, key: GamepadKey) {
        self.runtime.world_mut().input.gamepads[pad].set_down(key, false);
        if let Some(obj) = self.pad_object(pad, key) {
            let zero = Vector3::new(0.0, 0.0, 0.0);
            self.update_input_object(obj, UserInputState::End, zero, zero);
            self.deliver("InputEnded", obj, false);
        }
    }

    fn pad_object(&self, pad: usize, key: GamepadKey) -> Option<rook_dom::InstanceId> {
        let mut w = self.runtime.world_mut();
        let w = &mut *w;
        w.input.gamepads[pad].object(&mut w.dom, pad, key)
    }

    /// Update an InputObject in place, so scripts watching its Changed
    /// see each update.
    fn update_input_object(
        &self,
        obj: rook_dom::InstanceId,
        state: UserInputState,
        position: Vector3,
        delta: Vector3,
    ) {
        self.set_property(obj, "UserInputState", state.into());
        self.set_property(obj, "Position", Value::Vector3(position));
        self.set_property(obj, "Delta", Value::Vector3(delta));
    }

    fn set_uis_bool(&self, name: &str, on: bool) {
        if let Some(uis) = self.uis() {
            self.set_property(uis, name, Value::Bool(on));
        }
    }

    fn fire_gamepad_signal(&self, event: &'static str, pad: UserInputType) {
        if let Some(uis) = self.uis()
            && let Ok(item) = self.runtime.push_enum_item("UserInputType", pad.value())
        {
            self.fire(uis, event, item);
        }
    }
}
