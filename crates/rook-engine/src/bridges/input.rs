//! Keyboard and mouse input for a client DataModel: GUI first, then the
//! world. UserInputService events, with `gameProcessedEvent` true when GUI
//! took the input; the player's Mouse; ClickDetectors; and TextBox
//! typing.
//!
//! Event and property behavior follows creator-docs (UserInputService,
//! InputObject, Mouse, ClickDetector); GUI's rules are measured
//! (docs/behavior/gui/input.md).

mod cursor;
mod events;
mod gamepad;
mod gui;
mod input_type;
mod keys;
mod textbox;
mod world;

pub use gamepad::GamepadAxis;
pub use rook_lua::gamepads::GamepadKind;

use rook_dom::InstanceId;
use rook_types::{GamepadKey, UserInputState, UserInputType, Value, Vector3};
use rustc_hash::FxHashMap;

use crate::DataModel;
use input_type::Preferred;
use keys::key_code_value;

#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// `code` is an `Enum.KeyCode` item name ("A", "Space", "LeftShift").
    Key {
        code: String,
        down: bool,
    },
    /// Text the platform typed (a key, an input method's composition, a
    /// paste), which goes to the focused TextBox.
    Text {
        text: String,
    },
    /// Button 1 (left), 2 (right) or 3 (middle).
    MouseButton {
        button: u8,
        down: bool,
    },
    /// Cursor position in pixels from the viewport's top left.
    MouseMove {
        x: f32,
        y: f32,
    },
    /// The mouse moved by this many pixels while the window holds it
    /// ([`crate::MouseLock`]): the cursor stays put, but scripts see the
    /// movement's `Delta`.
    MouseMotion {
        dx: f32,
        dy: f32,
    },
    /// Wheel steps; positive is forward.
    Wheel {
        delta: f32,
    },
    /// The window gained (true) or lost focus.
    Focus {
        focused: bool,
    },
    /// A pad took slot `pad` (0 is Gamepad1), with the keys it has.
    GamepadConnected {
        pad: usize,
        kind: GamepadKind,
        supported: Vec<GamepadKey>,
    },
    GamepadDisconnected {
        pad: usize,
    },
    /// A pad's button went down or up. Triggers are axes.
    GamepadButton {
        pad: usize,
        key: GamepadKey,
        down: bool,
    },
    /// One of a pad's thumbstick axes or triggers moved to `value`.
    GamepadAxis {
        pad: usize,
        axis: GamepadAxis,
        value: f32,
    },
}

/// A point in window pixels, written `x,y` (command lines' `--click`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPoint {
    pub x: f32,
    pub y: f32,
}

impl std::str::FromStr for ScreenPoint {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let (x, y) = s.split_once(',').ok_or("expected x,y")?;
        let n = |v: &str| v.trim().parse::<f32>().map_err(|e| e.to_string());
        Ok(Self { x: n(x)?, y: n(y)? })
    }
}

impl ScreenPoint {
    /// Clicking here with button 1: the cursor moves here, then presses
    /// and releases.
    pub fn click(self) -> [InputEvent; 3] {
        [
            InputEvent::MouseMove {
                x: self.x,
                y: self.y,
            },
            InputEvent::MouseButton {
                button: 1,
                down: true,
            },
            InputEvent::MouseButton {
                button: 1,
                down: false,
            },
        ]
    }
}

/// What the cursor looks like: an image (Roblox's own or a script's
/// icon), the ClickDetector hand, one of the system's, or hidden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorLook {
    Hidden,
    /// The hand over a ClickDetector the player can reach, which no data
    /// file holds (docs/behavior/gui/input.md#the-cursor).
    DetectorHand,
    /// An image's content URL, drawn centered on the cursor. Roblox's own
    /// are 64 by 64 with the point at the center (Observed in the
    /// install's `textures/Cursors/KeyboardMouse`).
    Image(String),
    System(SystemCursor),
}

/// The platform's own cursors, which `rbxasset://SystemCursors/<name>`
/// names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemCursor {
    Arrow,
    PointingHand,
    IBeam,
    OpenHand,
    ClosedHand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickKind {
    Click,
    RightClick,
    HoverEnter,
    HoverLeave,
}

impl ClickKind {
    pub fn event(self) -> &'static str {
        match self {
            Self::Click => "MouseClick",
            Self::RightClick => "RightMouseClick",
            Self::HoverEnter => "MouseHoverEnter",
            Self::HoverLeave => "MouseHoverLeave",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickEvent {
    /// The ClickDetector, in the client DataModel.
    pub detector: InstanceId,
    pub kind: ClickKind,
}

/// Per-client input bookkeeping the engine keeps beside the World's state.
#[derive(Debug, Default)]
pub(crate) struct InputTracker {
    pub(crate) viewport: (u32, u32),
    /// InputObjects of held keys, reused for their end event.
    held: FxHashMap<u32, InstanceId>,
    /// The InputObjects mouse movement and the wheel pass to every
    /// InputChanged: Roblox reuses one per input and updates it (recorded:
    /// conformance input/mouse_move_object; the wheel's own object was
    /// observed the same way).
    mouse_movement: Option<InstanceId>,
    mouse_wheel: Option<InstanceId>,
    /// The Focus InputObject, reused for every focus change (measured).
    focus: Option<InstanceId>,
    hover: Option<InstanceId>,
    /// The detector under the cursor when buttons 1 and 2 went down.
    pressed: [Option<InstanceId>; 2],
    /// What the mouse last saw, and from where.
    under: Option<world::Under>,
    /// How many times the mouse has looked, for tests.
    #[cfg(test)]
    pub(crate) looks: u32,
    gui: gui::GuiInput,
    /// GUI took the last event (its `gameProcessedEvent`).
    processed: bool,
    /// Escape ended a TextBox's focus, so its release is processed too
    /// (measured; Enter's isn't).
    escape_ended_box: bool,
}

impl InputTracker {
    /// Drop what is kept about `id`, which is being freed.
    pub(crate) fn forget(&mut self, id: InstanceId) {
        self.held.retain(|_, h| *h != id);
        for slot in [
            &mut self.mouse_movement,
            &mut self.mouse_wheel,
            &mut self.focus,
            &mut self.hover,
        ]
        .into_iter()
        .chain(&mut self.pressed)
        {
            slot.take_if(|s| *s == id);
        }
        self.under.take_if(|u| u.names(id));
        self.gui.forget(id);
    }
}

/// A window point in GUI coordinates, below the top bar, as InputObjects
/// and the Mouse give it (measured: `GetMouseLocation` alone includes
/// the inset).
fn gui_point((x, y): (f32, f32)) -> (f32, f32) {
    (x, y - rook_gui::GUI_INSET)
}

#[cfg(test)]
mod tests;
