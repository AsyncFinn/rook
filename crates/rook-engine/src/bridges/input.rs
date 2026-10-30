//! Keyboard and mouse input for a client DataModel: GUI first, then the
//! world. UserInputService events, with `gameProcessedEvent` true when GUI
//! took the input; the player's Mouse; ClickDetectors; and TextBox
//! typing.
//!
//! Event and property behavior follows creator-docs (UserInputService,
//! InputObject, Mouse, ClickDetector); GUI's rules are measured
//! (docs/behavior/gui/input.md).

mod gui;
mod textbox;
mod world;

use rook_dom::InstanceId;
use rook_types::{UserInputState, UserInputType, Value, Vector3};

use crate::DataModel;

/// A window input event, in Roblox terms.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// `code` is an `Enum.KeyCode` item name ("A", "Space", "LeftShift").
    Key { code: String, down: bool },
    /// Text the platform typed (a key, an input method's composition, a
    /// paste), which goes to the focused TextBox.
    Text { text: String },
    /// Button 1 (left), 2 (right) or 3 (middle).
    MouseButton { button: u8, down: bool },
    /// Cursor position in pixels from the viewport's top left.
    MouseMove { x: f32, y: f32 },
    /// The mouse moved by this many pixels while the window holds it
    /// ([`crate::MouseLock`]): the cursor stays put, but scripts see the
    /// movement's `Delta`.
    MouseMotion { dx: f32, dy: f32 },
    /// Wheel steps; positive is forward.
    Wheel { delta: f32 },
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
/// icon), one of the system's, or hidden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorLook {
    Hidden,
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

const ARROW: &str = "rbxasset://textures/Cursors/KeyboardMouse/ArrowFarCursor.png";
const HAND: &str = "rbxasset://textures/Cursors/KeyboardMouse/ArrowCursor.png";
const IBEAM: &str = "rbxasset://textures/Cursors/KeyboardMouse/IBeamCursor.png";

impl CursorLook {
    /// What shows while this look's image loads, or when it can't: the
    /// system's nearest for Roblox's own (no install), the arrow for a
    /// script's icon.
    pub fn stand_in(&self) -> Option<CursorLook> {
        let CursorLook::Image(url) = self else {
            return None;
        };
        Some(match url.as_str() {
            ARROW => CursorLook::System(SystemCursor::Arrow),
            HAND => CursorLook::System(SystemCursor::PointingHand),
            IBEAM => CursorLook::System(SystemCursor::IBeam),
            _ => CursorLook::Image(ARROW.to_owned()),
        })
    }

    /// The look a script's icon names.
    fn icon(url: String) -> CursorLook {
        let system = match url.strip_prefix("rbxasset://SystemCursors/") {
            Some("Arrow") => SystemCursor::Arrow,
            Some("PointingHand") => SystemCursor::PointingHand,
            Some("IBeam") => SystemCursor::IBeam,
            Some("OpenHand") => SystemCursor::OpenHand,
            Some("ClosedHand") => SystemCursor::ClosedHand,
            _ => return CursorLook::Image(url),
        };
        CursorLook::System(system)
    }
}

/// A ClickDetector interaction the client detected.
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
    held: std::collections::HashMap<u32, InstanceId>,
    /// The InputObjects mouse movement and the wheel pass to every
    /// InputChanged: Roblox reuses one per input and updates it (recorded:
    /// conformance input/mouse_move_object; the wheel's own object was
    /// observed the same way).
    mouse_movement: Option<InstanceId>,
    mouse_wheel: Option<InstanceId>,
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

/// What `Mouse.KeyDown` passes for a key: the character it types, for the
/// keys that type one (creator-docs Mouse.KeyDown, deprecated).
fn legacy_key_string(code: &str) -> Option<String> {
    const DIGITS: [&str; 10] = [
        "Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine",
    ];
    if code.len() == 1 && code.as_bytes()[0].is_ascii_uppercase() {
        return Some(code.to_ascii_lowercase());
    }
    if let Some(d) = DIGITS.iter().position(|d| *d == code) {
        return Some(d.to_string());
    }
    Some(
        match code {
            "Space" => " ",
            "Return" => "\r",
            "Tab" => "\t",
            "Backspace" => "\u{8}",
            _ => return None,
        }
        .to_owned(),
    )
}

/// A window point in GUI coordinates, below the top bar, as InputObjects
/// and the Mouse give it (measured: `GetMouseLocation` alone includes
/// the inset).
fn gui_point((x, y): (f32, f32)) -> (f32, f32) {
    (x, y - rook_gui::GUI_INSET)
}

fn key_code_value(code: &str) -> Option<u32> {
    let e = rook_reflection::database().enum_by_name("KeyCode")?;
    rook_reflection::enum_item_value(e.id, code)
}

impl DataModel {
    /// What the cursor shows: none with `UserInputService.MouseIconEnabled`
    /// off, else `UserInputService.MouseIcon`, else the Mouse's `Icon`
    /// (Inference: UserInputService's wins when both are set). Else
    /// Roblox's hand over a GUI button, the system's pointing hand over a
    /// ClickDetector (Observed live), an I-beam over a TextBox
    /// (Inference), or Roblox's arrow (docs/behavior/gui/input.md#the-cursor).
    pub fn cursor_look(&self) -> CursorLook {
        let w = self.runtime.world();
        let uis = w.dom.find_service("UserInputService");
        if uis.is_some_and(|u| w.dom.bool_property(u, "MouseIconEnabled") == Some(false)) {
            return CursorLook::Hidden;
        }
        let icons = [
            uis.and_then(|u| w.dom.content_property(u, "MouseIconContent")),
            (w.input.mouse_object).and_then(|m| w.dom.content_property(m, "IconContent")),
        ];
        if let Some(url) = icons.into_iter().flatten().find(|url| !url.is_empty()) {
            return CursorLook::icon(url);
        }
        let image = match self.hovered_press_target() {
            Some(id) if w.dom.is_a(id, "TextBox") => IBEAM,
            Some(_) => HAND,
            None if self.input.hover.is_some() => {
                return CursorLook::System(SystemCursor::PointingHand);
            }
            None => ARROW,
        };
        CursorLook::Image(image.to_owned())
    }

    /// The image at `url` if the GUI has loaded it, asking for it the
    /// first time.
    pub fn gui_image(&self, url: &str) -> Option<rook_gui::GuiImage> {
        self.runtime.world_mut().gui.image(url)
    }

    /// Whether GUI took the last input event (its `gameProcessedEvent`):
    /// the window shouldn't also use it.
    pub fn input_processed(&self) -> bool {
        self.input.processed
    }

    /// Where the game reads the mouse, in window pixels: the last
    /// [`InputEvent::MouseMove`].
    pub fn mouse_position(&self) -> (f32, f32) {
        self.runtime.world().input.mouse
    }

    /// The TextBox keys are typing into, if any.
    pub fn focused_text_box(&self) -> Option<InstanceId> {
        self.runtime.focused_text_box()
    }

    /// The window's size in pixels (for the mouse ray and Mouse.ViewSize).
    pub fn set_viewport(&mut self, width: u32, height: u32) {
        self.input.viewport = (width.max(1), height.max(1));
        self.sync_viewport_size();
    }

    /// Give the current camera the window's size as its ViewportSize, which
    /// `Camera:ViewportPointToRay` and friends and GUI layout read. A
    /// change fires Changed, as a resize does in Roblox. Nothing happens
    /// before the host gives a window size.
    pub(crate) fn sync_viewport_size(&self) {
        let (w, h) = self.input.viewport;
        if (w, h) == (0, 0) {
            return;
        }
        let Some(camera) = self.current_camera() else {
            return;
        };
        let size = rook_types::Vector2::new(w as f32, h as f32);
        let current = self
            .runtime
            .world()
            .dom
            .vector2_property(camera, "ViewportSize");
        if current != Some(size) {
            self.set_property(camera, "ViewportSize", Value::Vector2(size));
        }
    }

    /// Create the local player's Mouse if it doesn't exist yet.
    pub(crate) fn ensure_mouse(&self) {
        let mut w = self.runtime.world_mut();
        if w.input.mouse_object.is_none()
            && let Ok(m) = w.dom.create_by_name("PlayerMouse")
        {
            w.input.mouse_object = Some(m);
        }
    }

    /// Fire one of UserInputService's input events.
    fn fire_input(&self, event: &'static str, object: InstanceId, processed: bool) {
        let uis = self
            .runtime
            .world_mut()
            .dom
            .get_service("UserInputService")
            .ok();
        if let (Some(uis), Ok(obj)) = (uis, self.runtime.push_instance(object)) {
            self.fire(uis, event, (obj, processed));
        }
    }

    /// Give ContextActionService's bindings what GUI didn't take, after
    /// UserInputService has heard of it (Inference: the order isn't
    /// measured).
    fn offer_action(&self, object: InstanceId, processed: bool) {
        if !processed {
            self.runtime.context_action(object);
        }
    }

    /// Look under the mouse again after a frame; the ClickDetector hovers
    /// it changed.
    pub fn refresh_hover(&mut self) -> Vec<ClickEvent> {
        let mut clicks = Vec::new();
        self.refresh_mouse(&mut clicks);
        clicks
    }

    fn mouse_object(&self) -> Option<InstanceId> {
        self.runtime.world().input.mouse_object
    }

    /// Handle one input event. Returns the ClickDetector interactions it
    /// caused, for the session to fire on both sides.
    pub fn input(&mut self, event: InputEvent) -> Vec<ClickEvent> {
        profiling::scope!("DataModel::input");
        self.ensure_mouse();
        let mut clicks = Vec::new();
        self.input.processed = match event {
            InputEvent::Key { code, down } => self.key(&code, down),
            InputEvent::Text { text } => self.type_text(&text),
            InputEvent::MouseButton { button, down } => {
                self.mouse_button(button, down, &mut clicks)
            }
            InputEvent::MouseMove { x, y } => self.mouse_move(x, y, &mut clicks),
            InputEvent::MouseMotion { dx, dy } => self.mouse_motion(dx, dy),
            InputEvent::Wheel { delta } => self.wheel(delta),
        };
        clicks
    }

    /// The cursor moved while the mouse is free. Observed: its
    /// InputChanged `Delta` and `GetMouseDelta` are zero then; only a held
    /// mouse ([`InputEvent::MouseMotion`]) moves by deltas, which the
    /// default camera turns by.
    fn mouse_move(&mut self, x: f32, y: f32, clicks: &mut Vec<ClickEvent>) -> bool {
        {
            let mut w = self.runtime.world_mut();
            w.input.mouse = (x, y);
            w.input.mouse_delta = (0.0, 0.0);
        }
        let (gx, gy) = gui_point((x, y));
        let obj =
            self.changed_input_object(UserInputType::MouseMovement, (gx, gy, 0.0), (0.0, 0.0));
        let processed = self.gui_move(obj);
        if let Some(obj) = obj {
            self.fire_input("InputChanged", obj, processed);
        }
        self.refresh_mouse(clicks);
        if let Some(m) = self.mouse_object() {
            self.fire(m, "Move", ());
        }
        processed
    }

    /// Movement of a held mouse: InputChanged with the movement in
    /// `Delta`, the cursor at the window's center for
    /// [`MouseLock::Center`](crate::MouseLock) (Inference: where Roblox
    /// draws it in first person), else where it was.
    fn mouse_motion(&mut self, dx: f32, dy: f32) -> bool {
        self.hold_mouse();
        let pos = {
            let mut w = self.runtime.world_mut();
            w.input.mouse_delta = (dx, dy);
            w.input.mouse
        };
        let (gx, gy) = gui_point(pos);
        let obj = self.changed_input_object(UserInputType::MouseMovement, (gx, gy, 0.0), (dx, dy));
        if let Some(obj) = obj {
            self.fire_input("InputChanged", obj, false);
        }
        false
    }

    /// Put the mouse where [`DataModel::mouse_lock`] holds it: at the
    /// window's center for [`MouseLock::Center`](crate::MouseLock), as soon
    /// as the lock comes (on each held move, and each client frame once
    /// scripts have set `MouseBehavior`), not where the cursor was when it
    /// came. The engine's alone: a window only reports where its cursor
    /// is. No signal fires (Inference).
    pub(crate) fn hold_mouse(&self) {
        if self.mouse_lock() != crate::MouseLock::Center {
            return;
        }
        let (vw, vh) = self.input.viewport;
        self.runtime.world_mut().input.mouse = (vw as f32 / 2.0, vh as f32 / 2.0);
    }

    fn wheel(&mut self, delta: f32) -> bool {
        let pos = gui_point(self.runtime.world().input.mouse);
        // The wheel amount is in Position.Z (creator-docs InputObject).
        let obj =
            self.changed_input_object(UserInputType::MouseWheel, (pos.0, pos.1, delta), (0.0, 0.0));
        let processed = self.gui_wheel(delta, obj);
        if let Some(obj) = obj {
            self.fire_input("InputChanged", obj, processed);
        }
        // Inference: after InputChanged, as Roblox's camera reads both.
        self.runtime
            .pointer_action(delta, (0.0, 0.0), 0.0, processed);
        if let Some(m) = self.mouse_object() {
            let event = if delta > 0.0 {
                "WheelForward"
            } else {
                "WheelBackward"
            };
            self.fire(m, event, ());
        }
        processed
    }

    fn new_input_object(
        &self,
        key: u32,
        input_type: UserInputType,
        state: UserInputState,
        pos: (f32, f32),
        delta: (f32, f32),
    ) -> Option<InstanceId> {
        let mut w = self.runtime.world_mut();
        rook_lua::input_object(&mut w.dom, key, input_type, state, pos, delta)
    }

    /// The InputObject of mouse movement or the wheel, made on first use
    /// and updated in place after that.
    fn changed_input_object(
        &mut self,
        input_type: UserInputType,
        pos: (f32, f32, f32),
        delta: (f32, f32),
    ) -> Option<InstanceId> {
        let slot = if input_type == UserInputType::MouseWheel {
            &mut self.input.mouse_wheel
        } else {
            &mut self.input.mouse_movement
        };
        // A script may have destroyed it.
        let existing = slot.filter(|id| !self.runtime.world().dom.is_destroyed(*id));
        let id = match existing {
            Some(id) => id,
            None => {
                let mut w = self.runtime.world_mut();
                let id = rook_lua::input_object(
                    &mut w.dom,
                    0,
                    input_type,
                    UserInputState::Change,
                    (pos.0, pos.1),
                    delta,
                )?;
                *slot = Some(id);
                id
            }
        };
        // In place, so scripts watching its Changed see each update.
        self.set_property(id, "UserInputState", UserInputState::Change.into());
        self.set_property(
            id,
            "Position",
            Value::Vector3(Vector3::new(pos.0, pos.1, pos.2)),
        );
        self.set_property(
            id,
            "Delta",
            Value::Vector3(Vector3::new(delta.0, delta.1, 0.0)),
        );
        Some(id)
    }

    /// A key going down or up. A focused TextBox takes it first, and then
    /// it reaches scripts as processed, but not the legacy Mouse.
    fn key(&mut self, code: &str, down: bool) -> bool {
        let Some(value) = key_code_value(code) else {
            return false;
        };
        let pos = gui_point(self.runtime.world().input.mouse);
        let mut processed = self.focused_text_box().is_some();
        if !down && code == "Escape" {
            processed |= std::mem::take(&mut self.input.escape_ended_box);
        }
        if down {
            // Auto-repeat doesn't begin the input again, but it repeats
            // a TextBox's editing keys (Inference).
            if !self.runtime.world_mut().input.keys.insert(value) {
                if processed && let Some(&obj) = self.input.held.get(&value) {
                    self.edit_key(code, obj);
                }
                return processed;
            }
            let Some(obj) = self.new_input_object(
                value,
                UserInputType::Keyboard,
                UserInputState::Begin,
                pos,
                (0.0, 0.0),
            ) else {
                return processed;
            };
            self.input.held.insert(value, obj);
            if processed {
                self.edit_key(code, obj);
            }
            self.fire_input("InputBegan", obj, processed);
            self.offer_action(obj, processed);
        } else {
            if !self.runtime.world_mut().input.keys.remove(&value) {
                return processed;
            }
            let obj = self.input.held.remove(&value).or_else(|| {
                self.new_input_object(
                    value,
                    UserInputType::Keyboard,
                    UserInputState::Begin,
                    pos,
                    (0.0, 0.0),
                )
            });
            let Some(obj) = obj else {
                return processed;
            };
            self.set_property(obj, "UserInputState", UserInputState::End.into());
            self.fire_input("InputEnded", obj, processed);
            self.offer_action(obj, processed);
        }
        if !processed && let (Some(m), Some(s)) = (self.mouse_object(), legacy_key_string(code)) {
            self.fire(m, if down { "KeyDown" } else { "KeyUp" }, s);
        }
        processed
    }

    fn mouse_button(&mut self, button: u8, down: bool, clicks: &mut Vec<ClickEvent>) -> bool {
        if !(1..=3).contains(&button) {
            return false;
        }
        let index = usize::from(button - 1);
        let pos = {
            let mut w = self.runtime.world_mut();
            w.input.buttons[index] = down;
            gui_point(w.input.mouse)
        };
        let state = if down {
            UserInputState::Begin
        } else {
            UserInputState::End
        };
        let obj = self.new_input_object(
            0,
            UserInputType::MOUSE_BUTTONS[index],
            state,
            pos,
            (0.0, 0.0),
        );
        // GUI hears of it first (measured).
        let processed = match (button, down) {
            (3, _) => self.gui_covers_active(),
            (_, true) => self.gui_press(button, obj),
            (_, false) => self.gui_release(button, obj),
        };
        if let Some(obj) = obj {
            self.fire_input(
                if down { "InputBegan" } else { "InputEnded" },
                obj,
                processed,
            );
            self.offer_action(obj, processed);
        }
        let detector = self.refresh_mouse(clicks);
        if let Some(m) = self.mouse_object()
            && button <= 2
        {
            let event = match (button, down) {
                (1, true) => "Button1Down",
                (1, false) => "Button1Up",
                (_, true) => "Button2Down",
                (_, false) => "Button2Up",
            };
            self.fire(m, event, ());
        }
        // A click is a press and release over the same detector
        // (creator-docs ClickDetector.MouseClick); measured, an Active
        // GUI object over it takes the click.
        if button <= 2 {
            let slot = &mut self.input.pressed[index];
            if down {
                *slot = detector.filter(|_| !processed);
            } else if let (Some(pressed), Some(now)) = (slot.take(), detector)
                && pressed == now
                && !processed
            {
                clicks.push(ClickEvent {
                    detector: now,
                    kind: if button == 1 {
                        ClickKind::Click
                    } else {
                        ClickKind::RightClick
                    },
                });
            }
        }
        processed
    }
}

#[cfg(test)]
mod tests {
    use super::legacy_key_string;

    #[test]
    fn legacy_key_strings() {
        assert_eq!(legacy_key_string("W").as_deref(), Some("w"));
        assert_eq!(legacy_key_string("Seven").as_deref(), Some("7"));
        assert_eq!(legacy_key_string("Space").as_deref(), Some(" "));
        assert_eq!(legacy_key_string("LeftShift"), None);
    }
}
