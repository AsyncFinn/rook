//! Keyboard and mouse input for a client DataModel: UserInputService
//! events, the player's Mouse, and ClickDetectors.
//!
//! Event and property behaviour follows creator-docs (UserInputService,
//! InputObject, Mouse, ClickDetector). Rook has no GUI yet, so
//! `gameProcessedEvent` is always false.

use glam::Vec3;
use rook_dom::InstanceId;
use rook_types::{CFrame, Value, Vector3};

use crate::DataModel;

/// A window input event, in Roblox terms.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// `code` is an `Enum.KeyCode` item name ("A", "Space", "LeftShift").
    Key { code: String, down: bool },
    /// Button 1 (left), 2 (right) or 3 (middle).
    MouseButton { button: u8, down: bool },
    /// Cursor position in pixels from the viewport's top left.
    MouseMove { x: f32, y: f32 },
    /// Wheel steps; positive is forward.
    Wheel { delta: f32 },
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
    hover: Option<InstanceId>,
    /// The detector under the cursor when buttons 1 and 2 went down.
    pressed: [Option<InstanceId>; 2],
}

// Enum.UserInputType and Enum.UserInputState values.
const TYPE_MOUSE_WHEEL: u32 = 3;
const TYPE_MOUSE_MOVEMENT: u32 = 4;
const TYPE_KEYBOARD: u32 = 8;
const STATE_BEGIN: u32 = 0;
const STATE_CHANGE: u32 = 1;
const STATE_END: u32 = 2;

/// How far the mouse looks for a target (Inference).
const MOUSE_RANGE: f32 = 1000.0;

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

fn key_code_value(code: &str) -> Option<u32> {
    let e = rook_reflection::database().enum_by_name("KeyCode")?;
    rook_reflection::enum_item_value(e.id, code)
}

impl DataModel {
    /// The ClickDetector under the cursor, which the window shows as a
    /// pointing hand.
    pub fn hovered_click_detector(&self) -> Option<InstanceId> {
        self.input.hover
    }

    /// The window's size in pixels (for the mouse ray and Mouse.ViewSize).
    pub fn set_viewport(&mut self, width: u32, height: u32) {
        self.input.viewport = (width.max(1), height.max(1));
        self.sync_viewport_size();
    }

    /// Give the current camera the window's size as its ViewportSize, which
    /// `Camera:ViewportPointToRay` and friends read.
    fn sync_viewport_size(&self) {
        let Some(camera) = self.current_camera() else {
            return;
        };
        let (w, h) = self.input.viewport;
        let size = Value::Vector2(rook_types::Vector2::new(w as f32, h as f32));
        let mut world = self.runtime.world_mut();
        if world.dom.property_by_name(camera, "ViewportSize").as_ref() != Some(&size) {
            set(&mut world.dom, camera, "ViewportSize", size);
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

    fn fire_input(&self, event: &'static str, object: InstanceId) {
        let uis = self
            .runtime
            .world_mut()
            .dom
            .get_service("UserInputService")
            .ok();
        if let (Some(uis), Ok(obj)) = (uis, self.runtime.push_instance(object)) {
            self.fire(uis, event, (obj, false));
        }
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
        match event {
            InputEvent::Key { code, down } => self.key(&code, down),
            InputEvent::MouseButton { button, down } => {
                self.mouse_button(button, down, &mut clicks);
            }
            InputEvent::MouseMove { x, y } => {
                let (dx, dy) = {
                    let mut w = self.runtime.world_mut();
                    let (ox, oy) = w.input.mouse;
                    w.input.mouse = (x, y);
                    w.input.mouse_delta = (x - ox, y - oy);
                    w.input.mouse_delta
                };
                let obj =
                    self.new_input_object(0, TYPE_MOUSE_MOVEMENT, STATE_CHANGE, (x, y), (dx, dy));
                if let Some(obj) = obj {
                    self.fire_input("InputChanged", obj);
                }
                self.refresh_mouse(&mut clicks);
                if let Some(m) = self.mouse_object() {
                    self.fire(m, "Move", ());
                }
            }
            InputEvent::Wheel { delta } => {
                let pos = self.runtime.world().input.mouse;
                // The wheel amount is in Position.Z (creator-docs InputObject).
                let obj = {
                    let mut w = self.runtime.world_mut();
                    let id = rook_lua::input_object(
                        &mut w.dom,
                        0,
                        TYPE_MOUSE_WHEEL,
                        STATE_CHANGE,
                        pos,
                        (0.0, 0.0),
                    );
                    if let Some(id) = id {
                        set(
                            &mut w.dom,
                            id,
                            "Position",
                            Value::Vector3(Vector3::new(pos.0, pos.1, delta)),
                        );
                    }
                    id
                };
                if let Some(obj) = obj {
                    self.fire_input("InputChanged", obj);
                }
                if let Some(m) = self.mouse_object() {
                    self.fire(
                        m,
                        if delta > 0.0 {
                            "WheelForward"
                        } else {
                            "WheelBackward"
                        },
                        (),
                    );
                }
            }
        }
        clicks
    }

    fn new_input_object(
        &self,
        key: u32,
        input_type: u32,
        state: u32,
        pos: (f32, f32),
        delta: (f32, f32),
    ) -> Option<InstanceId> {
        let mut w = self.runtime.world_mut();
        rook_lua::input_object(&mut w.dom, key, input_type, state, pos, delta)
    }

    fn key(&mut self, code: &str, down: bool) {
        let Some(value) = key_code_value(code) else {
            return;
        };
        let pos = self.runtime.world().input.mouse;
        if down {
            // Auto-repeat doesn't begin the input again.
            if !self.runtime.world_mut().input.keys.insert(value) {
                return;
            }
            let Some(obj) =
                self.new_input_object(value, TYPE_KEYBOARD, STATE_BEGIN, pos, (0.0, 0.0))
            else {
                return;
            };
            self.input.held.insert(value, obj);
            self.fire_input("InputBegan", obj);
        } else {
            if !self.runtime.world_mut().input.keys.remove(&value) {
                return;
            }
            let obj = self.input.held.remove(&value).or_else(|| {
                self.new_input_object(value, TYPE_KEYBOARD, STATE_BEGIN, pos, (0.0, 0.0))
            });
            let Some(obj) = obj else {
                return;
            };
            self.set_property(obj, "UserInputState", Value::Enum(STATE_END));
            self.fire_input("InputEnded", obj);
        }
        if let (Some(m), Some(s)) = (self.mouse_object(), legacy_key_string(code)) {
            self.fire(m, if down { "KeyDown" } else { "KeyUp" }, s);
        }
    }

    fn mouse_button(&mut self, button: u8, down: bool, clicks: &mut Vec<ClickEvent>) {
        if !(1..=3).contains(&button) {
            return;
        }
        let index = usize::from(button - 1);
        let pos = {
            let mut w = self.runtime.world_mut();
            w.input.buttons[index] = down;
            w.input.mouse
        };
        let state = if down { STATE_BEGIN } else { STATE_END };
        if let Some(obj) = self.new_input_object(0, u32::from(button - 1), state, pos, (0.0, 0.0)) {
            self.fire_input(if down { "InputBegan" } else { "InputEnded" }, obj);
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
        // (creator-docs ClickDetector.MouseClick).
        if button <= 2 {
            let slot = &mut self.input.pressed[index];
            if down {
                *slot = detector;
            } else if let (Some(pressed), Some(now)) = (slot.take(), detector)
                && pressed == now
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
    }

    /// The ray from the camera through the cursor (unit direction), the
    /// same as `Camera:ViewportPointToRay` but starting at the camera.
    fn mouse_ray(&self) -> Option<(Vec3, Vec3, CFrame)> {
        let camera = self.current_camera()?;
        let w = self.runtime.world();
        let (vw, vh) = self.input.viewport;
        let lens = rook_lua::Lens {
            viewport: glam::Vec2::new(vw as f32, vh as f32),
            ..rook_lua::Lens::of(&w.dom, camera)
        };
        let (mx, my) = w.input.mouse;
        let (_, dir) = rook_lua::viewport_ray(&lens, mx, my);
        Some((lens.cframe.position, dir, lens.cframe))
    }

    /// Update Mouse.Hit, Target and the rest from the cursor and camera,
    /// and track which ClickDetector is under the cursor. Returns it.
    pub(crate) fn refresh_mouse(&mut self, clicks: &mut Vec<ClickEvent>) -> Option<InstanceId> {
        let mouse = self.mouse_object()?;
        // The camera may have been replaced since the window last resized.
        self.sync_viewport_size();
        let (origin, dir, camera) = self.mouse_ray()?;
        let (hit, click_hit) = {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            // TargetFilter and what it contains are ignored; otherwise the
            // mouse sees what legacy rays see (Inference).
            let ignore = w
                .dom
                .ref_property(mouse, "TargetFilter")
                .into_iter()
                .collect();
            let filter = rook_physics::Filter::legacy_ray(ignore, false);
            let hit = w
                .physics
                .raycast(&mut w.dom, origin, dir * MOUSE_RANGE, &filter);
            // Clicks pass through invisible parts that don't collide; the
            // same TargetFilter applying is Inference.
            let clicks = rook_physics::Filter {
                skip_invisible_ghosts: true,
                ..filter
            };
            let click_hit = w
                .physics
                .raycast(&mut w.dom, origin, dir * MOUSE_RANGE, &clicks);
            (hit, click_hit)
        };
        let point = hit.map_or(origin + dir * MOUSE_RANGE, |h| h.position);
        let (mx, my) = self.runtime.world().input.mouse;
        let (vw, vh) = self.input.viewport;
        let hit_cf = CFrame {
            position: point,
            rotation: camera.rotation,
        };
        self.set_property(mouse, "Hit", Value::CFrame(hit_cf));
        self.set_property(mouse, "Origin", Value::CFrame(camera));
        self.set_property(mouse, "Target", Value::Ref(hit.map(|h| h.part.to_ref())));
        self.set_property(mouse, "X", Value::Int32(mx as i32));
        self.set_property(mouse, "Y", Value::Int32(my as i32));
        self.set_property(mouse, "ViewSizeX", Value::Int32(vw as i32));
        self.set_property(mouse, "ViewSizeY", Value::Int32(vh as i32));
        self.set_property(
            mouse,
            "UnitRay",
            Value::Ray(rook_types::Ray::new(
                Vector3(origin).into(),
                Vector3(dir).into(),
            )),
        );

        let detector =
            click_hit.and_then(|h| self.detector_for(h.part, origin.distance(h.position)));
        if detector != self.input.hover {
            if let Some(old) = self.input.hover {
                clicks.push(ClickEvent {
                    detector: old,
                    kind: ClickKind::HoverLeave,
                });
            }
            if let Some(new) = detector {
                clicks.push(ClickEvent {
                    detector: new,
                    kind: ClickKind::HoverEnter,
                });
            }
            self.input.hover = detector;
        }
        detector
    }

    /// The ClickDetector that handles clicks on `part`: one inside the part,
    /// or inside the nearest Model containing it, if within its
    /// MaxActivationDistance. Distance is measured from the camera until
    /// characters exist (Inference; Roblox measures from the character).
    fn detector_for(&self, part: InstanceId, distance: f32) -> Option<InstanceId> {
        let w = self.runtime.world();
        let dom = &w.dom;
        let find = |owner: InstanceId| dom.find_first_child_of_class(owner, "ClickDetector");
        let detector = find(part).or_else(|| {
            dom.ancestors(part)
                .filter(|a| dom.is_a(*a, "Model"))
                .find_map(find)
        })?;
        let max = match dom.property_by_name(detector, "MaxActivationDistance") {
            Some(Value::Float32(m)) => m,
            _ => 32.0,
        };
        (distance <= max).then_some(detector)
    }
}

fn set(dom: &mut rook_dom::Dom, id: InstanceId, name: &str, value: Value) {
    if let Some(p) = dom.property_id(id, name) {
        dom.load_property(id, p, value);
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
