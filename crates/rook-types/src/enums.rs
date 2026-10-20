//! Roblox enums that more than one place in Rook reads or writes by value.
//! An enum read in one place matches its item names instead
//! (`rook_dom::Dom::enum_item_property`). Every item's value is checked
//! against the reflection database by this module's tests.

use crate::Value;

macro_rules! roblox_enum {
    ($(#[$meta:meta])* $name:ident { $($item:ident = $value:literal,)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($item = $value,)*
        }

        impl $name {
            /// The Roblox enum's name.
            pub const NAME: &str = stringify!($name);
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
    }
}

impl UserInputType {
    /// Mouse buttons 1 to 3, in order.
    pub const MOUSE_BUTTONS: [Self; 3] =
        [Self::MouseButton1, Self::MouseButton2, Self::MouseButton3];
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
        check(MeshType::NAME, MeshType::ITEMS);
        check(RollOffMode::NAME, RollOffMode::ITEMS);
    }
}
