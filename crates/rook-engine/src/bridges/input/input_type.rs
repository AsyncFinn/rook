//! Which input is in use: `GetLastInputType`, `LastInputTypeChanged` and
//! `PreferredInput`, and the window's focus as an input of its own
//! (docs/behavior/input/gamepads.md#which-input-is-in-use).

use rook_dom::InstanceId;
use rook_types::{UserInputState, UserInputType, Value};

use crate::DataModel;

/// The `Enum.PreferredInput` items an input can turn it to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Preferred {
    KeyboardAndMouse,
    Gamepad,
}

impl Preferred {
    fn name(self) -> &'static str {
        match self {
            Self::KeyboardAndMouse => "KeyboardAndMouse",
            Self::Gamepad => "Gamepad",
        }
    }
}

impl DataModel {
    /// Note that an input of `kind` came, before its own events fire:
    /// `LastInputTypeChanged` when the type differs, and `PreferredInput`
    /// for the inputs that set it (measured: keys and mouse buttons
    /// KeyboardAndMouse, a pad's counted input Gamepad, mouse movement,
    /// the wheel and focus nothing). The one place both change.
    pub(super) fn note_input_type(&self, kind: UserInputType, preferred: Option<Preferred>) {
        // A client starts at Focus, as GetLastInputType reads it.
        let was = self.runtime.world_mut().input.last_input_type.replace(kind);
        if was.unwrap_or(UserInputType::Focus) != kind
            && let Some(uis) = self.uis()
            && let Ok(item) = self.runtime.push_enum_item("UserInputType", kind.value())
        {
            self.fire(uis, "LastInputTypeChanged", item);
        }
        if let Some(preferred) = preferred {
            self.set_preferred_input(preferred);
        }
    }

    /// Set `PreferredInput`, firing its change only when it differs.
    pub(super) fn set_preferred_input(&self, preferred: Preferred) {
        let Some(uis) = self.uis() else {
            return;
        };
        let current = self
            .runtime
            .world()
            .dom
            .enum_item_property(uis, "PreferredInput");
        if current != Some(preferred.name())
            && let Some(value) = enum_value("PreferredInput", preferred.name())
        {
            self.set_property(uis, "PreferredInput", Value::Enum(value));
        }
    }

    pub(super) fn uis(&self) -> Option<InstanceId> {
        self.runtime.world().dom.find_service("UserInputService")
    }

    /// The window gained or lost focus: an input of type Focus, through
    /// one InputObject reused for each (measured), then `WindowFocused` or
    /// `WindowFocusReleased` (measured for losing it; Inference that
    /// gaining it is the same order).
    pub(super) fn focus(&mut self, focused: bool) -> bool {
        self.note_input_type(UserInputType::Focus, None);
        let state = if focused {
            UserInputState::Begin
        } else {
            UserInputState::End
        };
        let existing = (self.input.focus).filter(|id| !self.runtime.world().dom.is_destroyed(*id));
        let obj = existing.or_else(|| {
            let mut w = self.runtime.world_mut();
            rook_lua::input_object(
                &mut w.dom,
                0,
                UserInputType::Focus,
                state,
                (0.0, 0.0),
                (0.0, 0.0),
            )
        });
        let Some(obj) = obj else {
            return false;
        };
        self.input.focus = Some(obj);
        self.set_property(obj, "UserInputState", state.into());
        let event = if focused { "InputBegan" } else { "InputEnded" };
        let processed = self.deliver(event, obj, false);
        if let Some(uis) = self.uis() {
            let window = if focused {
                "WindowFocused"
            } else {
                "WindowFocusReleased"
            };
            self.fire(uis, window, ());
        }
        processed
    }
}

/// The value of `Enum.<enum_name>.<item>`.
pub(super) fn enum_value(enum_name: &str, item: &str) -> Option<u32> {
    let e = rook_reflection::database().enum_by_name(enum_name)?;
    rook_reflection::enum_item_value(e.id, item)
}
