//! Gamepad slots as UserInputService sees them: whether each is
//! connected, what it has, and the InputObjects every event and
//! `GetGamepadState` share (docs/behavior/input/gamepads.md).

use rook_dom::{Dom, InstanceId};
use rook_types::{GamepadKey, UserInputState, UserInputType};

pub const GAMEPAD_SLOTS: usize = 8;

/// Which buttons' names and pictures a pad has: UserInputService names
/// keys after the kind of pad in use (measured).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GamepadKind {
    /// Any other pad, and what's named before any pad connects.
    #[default]
    Default,
    Xbox,
    PlayStation4,
    PlayStation5,
}

#[derive(Debug, Clone, Default)]
pub struct Gamepad {
    pub connected: bool,
    pub kind: GamepadKind,
    /// What the pad has, in `GetSupportedGamepadKeyCodes`' order; a
    /// thumbstick is listed once per axis, as Studio lists it.
    pub supported: Vec<GamepadKey>,
    /// Keys between InputBegan and InputEnded, by [`GamepadKey::index`].
    pub down: u32,
    /// Thumbstick 1 and 2's positions, right and up positive.
    pub sticks: [(f32, f32); 2],
    /// ButtonL2's and ButtonR2's pull, 0 to 1.
    pub triggers: [f32; 2],
    /// `SetNavigationGamepad(pad, false)` took it off the navigation
    /// list, which holds every slot until then (measured).
    pub not_navigation: bool,
    /// Its InputObjects, by [`GamepadKey::index`], made on first use and
    /// reused by every event and `GetGamepadState`, as Studio's are.
    objects: [Option<InstanceId>; GamepadKey::COUNT],
}

impl Gamepad {
    pub fn is_down(&self, key: GamepadKey) -> bool {
        self.down & (1 << key.index()) != 0
    }

    pub fn set_down(&mut self, key: GamepadKey, down: bool) {
        let bit = 1 << key.index();
        if down {
            self.down |= bit;
        } else {
            self.down &= !bit;
        }
    }

    /// The InputObject this slot's `key` reports through: Studio's End,
    /// at zero, until an input moves it.
    pub fn object(&mut self, dom: &mut Dom, pad: usize, key: GamepadKey) -> Option<InstanceId> {
        let slot = &mut self.objects[key.index()];
        // A script may have destroyed it; Studio's own would live on, so
        // a new one stands for it.
        if let Some(id) = *slot
            && dom.get(id).is_some_and(|i| !i.is_destroyed())
        {
            return Some(id);
        }
        let id = crate::input_object(
            dom,
            key.value(),
            UserInputType::GAMEPADS[pad],
            UserInputState::End,
            (0.0, 0.0),
            (0.0, 0.0),
        )?;
        *slot = Some(id);
        Some(id)
    }

    /// Every key's InputObject, in `GetGamepadState`'s order.
    pub fn objects(&mut self, dom: &mut Dom, pad: usize) -> Vec<InstanceId> {
        (GamepadKey::ITEMS.iter())
            .filter_map(|&(_, key)| self.object(dom, pad, key))
            .collect()
    }

    /// Drop `id` if it's one of this slot's objects, which is being freed.
    pub fn forget(&mut self, id: InstanceId) {
        for slot in &mut self.objects {
            slot.take_if(|s| *s == id);
        }
    }
}
