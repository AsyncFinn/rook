//! Roblox enums that more than one place in Rook reads or writes by value.
//! An enum read in one place matches its item names instead
//! (`rook_dom::Dom::enum_item_property`). Every item's value is checked
//! against the reflection database by this module's tests.

use crate::Value;

macro_rules! roblox_enum {
    ($(#[$meta:meta])* $name:ident { $($item:ident = $value:literal,)* }) => {
        roblox_enum!(@ $(#[$meta])* $name, stringify!($name), { $($item = $value,)* });
    };
    // Some of a Roblox enum's items, under a name of their own.
    ($(#[$meta:meta])* $name:ident as $roblox:literal { $($item:ident = $value:literal,)* }) => {
        roblox_enum!(@ $(#[$meta])* $name, $roblox, { $($item = $value,)* });
    };
    (@ $(#[$meta:meta])* $name:ident, $roblox:expr, { $($item:ident = $value:literal,)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($item = $value,)*
        }

        impl $name {
            /// The Roblox enum's name.
            pub const NAME: &str = $roblox;
            /// The items Rook names, by their Roblox names.
            pub const ITEMS: &[(&str, $name)] = &[$((stringify!($item), Self::$item),)*];

            /// The item with this value, if Rook names it.
            pub fn from_u32(value: u32) -> Option<Self> {
                match value {
                    $($value => Some(Self::$item),)*
                    _ => None,
                }
            }

            pub fn value(self) -> u32 {
                self as u32
            }
        }

        impl From<$name> for Value {
            fn from(item: $name) -> Self {
                Value::Enum(item.value())
            }
        }
    };
}

roblox_enum! {
    /// `Enum.RunContext`: where a Script runs.
    RunContext {
        Legacy = 0,
        Server = 1,
        Client = 2,
        Plugin = 3,
    }
}

roblox_enum! {
    /// `Enum.UserInputType`, the inputs Rook delivers.
    UserInputType {
        MouseButton1 = 0,
        MouseButton2 = 1,
        MouseButton3 = 2,
        MouseWheel = 3,
        MouseMovement = 4,
        Keyboard = 8,
        Focus = 9,
        Gamepad1 = 12,
        Gamepad2 = 13,
        Gamepad3 = 14,
        Gamepad4 = 15,
        Gamepad5 = 16,
        Gamepad6 = 17,
        Gamepad7 = 18,
        Gamepad8 = 19,
        None = 22,
    }
}

impl UserInputType {
    /// Mouse buttons 1 to 3, in order.
    pub const MOUSE_BUTTONS: [Self; 3] =
        [Self::MouseButton1, Self::MouseButton2, Self::MouseButton3];

    /// Gamepad1 to Gamepad8, in order.
    pub const GAMEPADS: [Self; 8] = [
        Self::Gamepad1,
        Self::Gamepad2,
        Self::Gamepad3,
        Self::Gamepad4,
        Self::Gamepad5,
        Self::Gamepad6,
        Self::Gamepad7,
        Self::Gamepad8,
    ];

    /// Which gamepad this is, 0 for Gamepad1, if it's one.
    pub fn gamepad_index(self) -> Option<usize> {
        Self::GAMEPADS.iter().position(|&g| g == self)
    }
}

roblox_enum! {
    /// The `Enum.KeyCode`s a gamepad has, in the order Studio's
    /// `GetGamepadState` lists them (docs/behavior/input/gamepads.md).
    GamepadKey as "KeyCode" {
        ButtonStart = 1010,
        ButtonA = 1002,
        ButtonSelect = 1011,
        ButtonB = 1003,
        ButtonX = 1000,
        ButtonY = 1001,
        ButtonL1 = 1005,
        ButtonR1 = 1004,
        ButtonL2 = 1007,
        ButtonR2 = 1006,
        ButtonL3 = 1009,
        ButtonUp = 1053,
        ButtonR3 = 1008,
        ButtonLeft = 1055,
        DPadUp = 1014,
        DPadDown = 1015,
        DPadLeft = 1012,
        DPadRight = 1013,
        Thumbstick1 = 1016,
        Thumbstick2 = 1017,
        ButtonCenter = 1051,
        ButtonBack = 1052,
        ButtonDown = 1054,
        ButtonRight = 1056,
    }
}

impl GamepadKey {
    /// How many keys a gamepad has.
    pub const COUNT: usize = Self::ITEMS.len();

    /// Where this key is in [`GamepadKey::ITEMS`].
    pub fn index(self) -> usize {
        Self::ITEMS
            .iter()
            .position(|&(_, k)| k == self)
            .expect("every key is an item")
    }

    /// The item's name.
    pub fn name(self) -> &'static str {
        Self::ITEMS[self.index()].0
    }

    /// A trigger: an axis with an analog pull, not a button.
    pub fn is_trigger(self) -> bool {
        matches!(self, Self::ButtonL2 | Self::ButtonR2)
    }

    pub fn is_thumbstick(self) -> bool {
        matches!(self, Self::Thumbstick1 | Self::Thumbstick2)
    }
}

roblox_enum! {
    /// `Enum.UserInputState`.
    UserInputState {
        Begin = 0,
        Change = 1,
        End = 2,
        Cancel = 3,
        None = 4,
    }
}

roblox_enum! {
    /// `Enum.RollOffMode`: how a Sound fades with distance.
    RollOffMode {
        Inverse = 0,
        Linear = 1,
        LinearSquare = 2,
        InverseTapered = 3,
    }
}

roblox_enum! {
    /// `Enum.MeshType`: what a SpecialMesh draws.
    MeshType {
        Head = 0,
        Torso = 1,
        Wedge = 2,
        Sphere = 3,
        Cylinder = 4,
        FileMesh = 5,
        Brick = 6,
        Prism = 7,
        Pyramid = 8,
        ParallelRamp = 9,
        RightAngleRamp = 10,
        CornerWedge = 11,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check<T: Copy + Into<Value>>(name: &str, items: &[(&str, T)]) {
        let db = rook_reflection::database();
        let info = db
            .enum_by_name(name)
            .unwrap_or_else(|| panic!("Enum.{name}"));
        for &(item, value) in items {
            let Value::Enum(value) = value.into() else {
                unreachable!()
            };
            assert!(
                info.items.contains(&(item, value)),
                "Enum.{name}.{item} is not {value} in the database"
            );
        }
    }

    #[test]
    fn values_match_the_database() {
        check(RunContext::NAME, RunContext::ITEMS);
        check(UserInputType::NAME, UserInputType::ITEMS);
        check(UserInputState::NAME, UserInputState::ITEMS);
        check(GamepadKey::NAME, GamepadKey::ITEMS);
        check(MeshType::NAME, MeshType::ITEMS);
        check(RollOffMode::NAME, RollOffMode::ITEMS);
    }
}
