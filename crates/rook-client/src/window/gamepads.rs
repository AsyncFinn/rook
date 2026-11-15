//! Gamepads, given to the game as `InputEvent`s
//! (docs/behavior/input/gamepads.md). Each source reports a pad's whole
//! state; this module gives pads their slots and turns each new state
//! into the events for what changed. gilrs reads most pads; on Windows,
//! where its only backend doesn't list Sony's, `sony` reads those over HID.

#[cfg(windows)]
mod sony;
mod sony_reports;
mod system;

use rook_engine::{GamepadAxis, GamepadKind, InputEvent};
use rook_types::GamepadKey;

/// The keys Studio lists for a pad, in its order: the same for a
/// DualShock and an Xbox pad, a stick once per axis (measured).
const STUDIO_KEYS: [GamepadKey; 20] = {
    use GamepadKey::*;
    [
        ButtonA,
        ButtonB,
        ButtonSelect,
        DPadDown,
        DPadLeft,
        DPadRight,
        DPadUp,
        ButtonL1,
        ButtonL3,
        ButtonL2,
        Thumbstick1,
        Thumbstick1,
        ButtonR1,
        ButtonR3,
        ButtonR2,
        Thumbstick2,
        Thumbstick2,
        ButtonStart,
        ButtonX,
        ButtonY,
    ]
};

/// What a pad is pressing and pointing at. Stick Y is up positive.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct PadState {
    /// Buttons down, by [`GamepadKey::index`]; triggers are `triggers`.
    buttons: u32,
    sticks: [(f32, f32); 2],
    triggers: [f32; 2],
}

impl PadState {
    fn set(&mut self, key: GamepadKey, down: bool) {
        let bit = 1 << key.index();
        if down {
            self.buttons |= bit;
        } else {
            self.buttons &= !bit;
        }
    }

    fn is_down(&self, key: GamepadKey) -> bool {
        self.buttons & (1 << key.index()) != 0
    }
}

/// Which source a pad came from, and its id there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PadId {
    System(gilrs::GamepadId),
    #[cfg_attr(not(windows), allow(dead_code))]
    Sony(u64),
}

#[derive(Debug)]
enum PadChange {
    Connected {
        id: PadId,
        kind: GamepadKind,
        supported: Vec<GamepadKey>,
    },
    State(PadId, PadState),
    Disconnected(PadId),
}

/// A pad in a slot, and the state the game was last told.
#[derive(Debug)]
struct Slot {
    id: PadId,
    sent: PadState,
}

#[derive(Debug)]
pub(crate) struct Gamepads {
    system: system::System,
    #[cfg(windows)]
    sony: Option<sony::Sony>,
    slots: [Option<Slot>; 8],
    changes: Vec<PadChange>,
}

impl Gamepads {
    pub(crate) fn new() -> Self {
        Self {
            system: system::System::new(),
            #[cfg(windows)]
            sony: sony::Sony::new()
                .inspect_err(|e| tracing::warn!("PlayStation pads won't be read: {e}"))
                .ok(),
            slots: Default::default(),
            changes: Vec::new(),
        }
    }

    /// What the pads did since the last read, in order. An unfocused
    /// window hears nothing from them (measured); a button let go still
    /// comes, so none stays held, and the rest comes with the pad's first
    /// report after focus returns (Inference).
    pub(crate) fn read(&mut self, focused: bool, mut send: impl FnMut(InputEvent)) {
        let mut changes = std::mem::take(&mut self.changes);
        self.system.poll(&mut changes);
        #[cfg(windows)]
        if let Some(sony) = &mut self.sony {
            sony.poll(&mut changes);
        }
        for change in changes.drain(..) {
            match change {
                PadChange::Connected {
                    id,
                    kind,
                    supported,
                } => {
                    self.connect(id, kind, supported, &mut send);
                }
                PadChange::Disconnected(id) => {
                    if let Some(pad) = self.slot_of(id) {
                        tracing::info!("gamepad {} disconnected", pad + 1);
                        self.slots[pad] = None;
                        send(InputEvent::GamepadDisconnected { pad });
                    }
                }
                PadChange::State(id, state) => {
                    if let Some(pad) = self.slot_of(id)
                        && let Some(slot) = &mut self.slots[pad]
                    {
                        diff(pad, &mut slot.sent, state, focused, &mut send);
                    }
                }
            }
        }
        self.changes = changes;
    }

    fn slot_of(&self, id: PadId) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.as_ref().is_some_and(|s| s.id == id))
    }

    /// The lowest free slot; Studio doesn't renumber the others when one
    /// leaves (measured).
    fn connect(
        &mut self,
        id: PadId,
        kind: GamepadKind,
        supported: Vec<GamepadKey>,
        send: &mut impl FnMut(InputEvent),
    ) {
        if self.slot_of(id).is_some() {
            return;
        }
        let Some(pad) = self.slots.iter().position(Option::is_none) else {
            return;
        };
        tracing::info!("gamepad {} connected: {kind:?}", pad + 1);
        self.slots[pad] = Some(Slot {
            id,
            sent: PadState::default(),
        });
        send(InputEvent::GamepadConnected {
            pad,
            kind,
            supported,
        });
    }
}

/// Send what changed from `sent` to `now`: buttons in `GetGamepadState`'s
/// order, then each stick's X and Y, then the triggers.
fn diff(
    pad: usize,
    sent: &mut PadState,
    now: PadState,
    focused: bool,
    send: &mut impl FnMut(InputEvent),
) {
    for &(_, key) in GamepadKey::ITEMS {
        let down = now.is_down(key);
        if sent.is_down(key) != down && (focused || !down) {
            sent.set(key, down);
            send(InputEvent::GamepadButton { pad, key, down });
        }
    }
    if !focused {
        return;
    }
    for i in 0..2 {
        let (x, y) = now.sticks[i];
        if sent.sticks[i].0 != x {
            sent.sticks[i].0 = x;
            let axis = GamepadAxis::StickX(i);
            send(InputEvent::GamepadAxis {
                pad,
                axis,
                value: x,
            });
        }
        if sent.sticks[i].1 != y {
            sent.sticks[i].1 = y;
            let axis = GamepadAxis::StickY(i);
            send(InputEvent::GamepadAxis {
                pad,
                axis,
                value: y,
            });
        }
    }
    for i in 0..2 {
        let value = now.triggers[i];
        if sent.triggers[i] != value {
            sent.triggers[i] = value;
            let axis = GamepadAxis::Trigger(i);
            send(InputEvent::GamepadAxis { pad, axis, value });
        }
    }
}

/// Which pad it is, which names its keys, by USB ids: Sony's DualShock 4
/// (both revisions and the wireless adapter) and DualSense (and Edge).
fn kind(vendor: u16, product: u16) -> GamepadKind {
    match (vendor, product) {
        (SONY, 0x05c4 | 0x09cc | 0x0ba0) => GamepadKind::PlayStation4,
        (SONY, 0x0ce6 | 0x0df2) => GamepadKind::PlayStation5,
        (0x045e, _) => GamepadKind::Xbox,
        _ => GamepadKind::Default,
    }
}

const SONY: u16 = 0x054c;

#[cfg(test)]
mod tests;
