//! The mouse on GUI: what it's over, `MouseEnter` and the rest, buttons'
//! presses, clicks and `Activated`, `GuiState`, the wheel on
//! ScrollingFrames and their dragged bars, and clicks focusing TextBoxes
//! (docs/behavior/gui/input.md).

use glam::Vec2;
use rook_dom::InstanceId;
use rook_types::{Value, Vector2, Viewer};

use super::gui_point;
use crate::DataModel;

/// Measured: activations this close count as a run for `Activated`'s
/// `clickCount` (0.465 s apart did, 0.534 s didn't).
const CLICK_RUN: f64 = 0.5;

#[derive(Debug, Default)]
pub(crate) struct GuiInput {
    /// What the cursor is over, top first.
    hovered: Vec<InstanceId>,
    /// The button mouse buttons 1 and 2 pressed.
    pressed: [Option<InstanceId>; 2],
    /// What got each held button's `InputBegan`, for its `InputEnded`.
    began: [Vec<InstanceId>; 2],
    drag: Option<rook_gui::BarDrag>,
    /// The last activation: its button, when, and its `clickCount`.
    last_activation: Option<(InstanceId, f64, i32)>,
    /// The GUI layout hover was last worked out on.
    hover_generation: u64,
}

impl DataModel {
    /// The GUI objects under the cursor that take the mouse, top first:
    /// on the screen, or else on the world GUI the mouse's ray meets
    /// first, unless a part is in front of it.
    fn gui_hits(&self) -> Vec<InstanceId> {
        let (x, y) = gui_point(self.runtime.world().input.mouse);
        let mut hits = {
            let w = self.runtime.world();
            w.gui.objects_at(&w.dom, Vec2::new(x, y))
        };
        if hits.is_empty() {
            hits = self.world_gui_hits();
        }
        let w = self.runtime.world();
        // Measured: non-interactable objects take nothing.
        hits.retain(|&id| w.dom.bool_property(id, "Interactable") != Some(false));
        hits
    }

    fn world_gui_hits(&self) -> Vec<InstanceId> {
        let Some((origin, dir, camera)) = self.mouse_ray() else {
            return Vec::new();
        };
        let height = self.input.viewport.1 as f32;
        let camera_id = self.current_camera();
        let mut w = self.runtime.world_mut();
        let w = &mut *w;
        let fov = camera_id
            .and_then(|c| w.dom.float_property(c, "FieldOfView"))
            .unwrap_or(70.0);
        let viewer = Viewer {
            position: origin,
            rotation: camera.rotation,
            fov_y: fov,
            height,
        };
        let Some(hit) = w.gui.world_objects_at(&w.dom, origin, dir, &viewer) else {
            return Vec::new();
        };
        if !hit.always_on_top {
            // Inference: parts in front hide a world GUI from the mouse as
            // they do from the eye.
            let filter = rook_physics::Filter::legacy_ray(Vec::new(), false);
            let reach = dir * (hit.distance - 0.05).max(0.0);
            if w.physics
                .raycast(&mut w.dom, origin, reach, &filter)
                .is_some()
            {
                return Vec::new();
            }
        }
        hit.objects
    }

    /// Whether GUI processes the mouse over `hits`: measured, when an
    /// `Active` object or a button (`Active` or not) is among them.
    fn any_active(&self, hits: &[InstanceId]) -> bool {
        let w = self.runtime.world();
        hits.iter().any(|&id| {
            w.dom.bool_property(id, "Active") == Some(true) || w.dom.is_a(id, "GuiButton")
        })
    }

    pub(super) fn gui_covers_active(&self) -> bool {
        let hits = self.gui_hits();
        self.any_active(&hits)
    }

    /// The window point mouse events on GUI pass: measured, it includes the
    /// top bar, unlike InputObjects'.
    fn window_point(&self) -> (i32, i32) {
        let (x, y) = self.runtime.world().input.mouse;
        (x as i32, y as i32)
    }

    fn fire_object_input(&self, id: InstanceId, event: &'static str, obj: Option<InstanceId>) {
        if let Some(obj) = obj
            && let Ok(obj) = self.runtime.push_instance(obj)
        {
            self.fire(id, event, obj);
        }
    }

    /// Set `id`'s `GuiState` from what the mouse is doing with it.
    fn update_gui_state(&self, id: InstanceId) {
        let state = {
            let w = self.runtime.world();
            if w.dom.get(id).is_none() {
                return;
            }
            let hovered = self.input.gui.hovered.contains(&id);
            let pressed = self.input.gui.pressed.contains(&Some(id));
            if w.dom.bool_property(id, "Interactable") == Some(false) {
                "NonInteractable"
            } else if hovered && pressed {
                "Press"
            } else if hovered {
                "Hover"
            } else {
                "Idle"
            }
        };
        let current = self.runtime.world().dom.enum_item_property(id, "GuiState");
        if current != Some(state)
            && let Some(value) = enum_value("GuiState", state)
        {
            self.set_property(id, "GuiState", value);
        }
    }

    /// Bring what the cursor is over up to date. Measured order:
    /// `MouseLeave`, `InputEnded` and `GuiState` on what it left, then
    /// `MouseEnter`, `InputBegan` and `GuiState` on what it entered, or,
    /// when the mouse `moved`, `MouseMoved` and `InputChanged` on what it
    /// stayed over. Returns what's under it.
    fn update_hover(&mut self, obj: Option<InstanceId>, moved: bool) -> Vec<InstanceId> {
        let hits = self.gui_hits();
        self.input.gui.hover_generation = self.runtime.world().gui.generation();
        let old = std::mem::replace(&mut self.input.gui.hovered, hits.clone());
        let at = self.window_point();
        for &id in old.iter().filter(|id| !hits.contains(id)) {
            if self.runtime.world().dom.get(id).is_none() {
                continue;
            }
            self.fire(id, "MouseLeave", at);
            self.fire_object_input(id, "InputEnded", obj);
            self.update_gui_state(id);
        }
        for &id in &hits {
            if !old.contains(&id) {
                self.fire(id, "MouseEnter", at);
                self.fire_object_input(id, "InputBegan", obj);
                self.update_gui_state(id);
            } else if moved {
                self.fire(id, "MouseMoved", at);
                self.fire_object_input(id, "InputChanged", obj);
            }
        }
        hits
    }

    /// GUI laid out again under a still cursor: what it's over changes as
    /// if it had moved there (Inference: Roblox checks each frame).
    pub(crate) fn refresh_gui_hover(&mut self) {
        let generation = self.runtime.world().gui.generation();
        if generation == self.input.gui.hover_generation {
            return;
        }
        let obj = self.input.mouse_movement;
        self.update_hover(obj, false);
    }

    /// The cursor moved: what it's over, then a held bar dragging its
    /// canvas, all before UserInputService's event.
    pub(super) fn gui_move(&mut self, obj: Option<InstanceId>) -> bool {
        let hits = self.update_hover(obj, true);
        if let Some(drag) = self.input.gui.drag {
            let (x, y) = gui_point(self.runtime.world().input.mouse);
            let to = self.runtime.world().gui.drag_bar(&drag, Vec2::new(x, y));
            if let Some(to) = to {
                self.set_property(drag.frame, "CanvasPosition", Value::Vector2(Vector2(to)));
            }
            return true;
        }
        self.any_active(&hits)
    }

    /// The first button or TextBox among `hits`, which takes a press.
    fn press_target(&self, hits: &[InstanceId]) -> Option<InstanceId> {
        let w = self.runtime.world();
        (hits.iter().copied()).find(|&id| w.dom.is_a(id, "GuiButton") || w.dom.is_a(id, "TextBox"))
    }

    /// The button or TextBox a press would go to now.
    pub(super) fn hovered_press_target(&self) -> Option<InstanceId> {
        self.press_target(&self.input.gui.hovered)
    }

    /// Mouse button 1 or 2 went down. Measured order: the top button's
    /// `MouseButton1Down`, `InputBegan` on everything under the cursor
    /// top first, then `GuiState` Press.
    pub(super) fn gui_press(&mut self, button: u8, obj: Option<InstanceId>) -> bool {
        let index = usize::from(button - 1);
        let hits = self.gui_hits();
        let target = self.press_target(&hits);
        let is_box = target.is_some_and(|t| self.runtime.world().dom.is_a(t, "TextBox"));
        // Measured: clicking elsewhere takes a TextBox's focus, passing
        // the click's InputObject.
        if button == 1
            && let Some(focused) = self.focused_text_box()
            && target != Some(focused)
        {
            self.runtime.release_text_box(false, obj);
            self.runtime.drain_deferred();
        }
        let (x, y) = gui_point(self.runtime.world().input.mouse);
        if button == 1
            && let Some(&top) = hits.first()
        {
            self.input.gui.drag = self.runtime.world().gui.grab_bar(top, Vec2::new(x, y));
        }
        let button_target = target.filter(|_| !is_box);
        let at = self.window_point();
        if let Some(t) = button_target {
            let event = if button == 1 {
                "MouseButton1Down"
            } else {
                "MouseButton2Down"
            };
            self.fire(t, event, at);
        }
        for &id in &hits {
            self.fire_object_input(id, "InputBegan", obj);
        }
        self.input.gui.began[index] = hits.clone();
        if let Some(t) = button_target {
            self.input.gui.pressed[index] = Some(t);
            self.update_gui_state(t);
        }
        if button == 1
            && is_box
            && let Some(t) = target
        {
            self.focus_by_click(t, Vec2::new(x, y));
        }
        self.input.gui.drag.is_some() || self.any_active(&hits)
    }

    /// Mouse button 1 or 2 came up. Measured order over the pressed
    /// button: `MouseButton1Click`, `MouseButton1Up`, `Activated`,
    /// `InputEnded`, then `GuiState`. Over another button, that one gets
    /// `MouseButton1Up` alone.
    pub(super) fn gui_release(&mut self, button: u8, obj: Option<InstanceId>) -> bool {
        let index = usize::from(button - 1);
        let hits = self.gui_hits();
        let pressed = self.input.gui.pressed[index].take();
        let over = self
            .press_target(&hits)
            .filter(|&t| self.runtime.world().dom.is_a(t, "GuiButton"));
        let at = self.window_point();
        let (click, up) = if button == 1 {
            ("MouseButton1Click", "MouseButton1Up")
        } else {
            ("MouseButton2Click", "MouseButton2Up")
        };
        if let Some(t) = over {
            let clicked = pressed == Some(t);
            if clicked {
                self.fire(t, click, ());
            }
            self.fire(t, up, at);
            if clicked
                && button == 1
                && let Some(obj) = obj
            {
                let count = self.click_count(t);
                if let Ok(obj) = self.runtime.push_instance(obj) {
                    self.fire(t, "Activated", (obj, count));
                }
            }
        }
        for id in std::mem::take(&mut self.input.gui.began[index]) {
            if self.runtime.world().dom.get(id).is_some() {
                self.fire_object_input(id, "InputEnded", obj);
            }
        }
        if let Some(p) = pressed {
            self.update_gui_state(p);
        }
        let dragged = button == 1 && self.input.gui.drag.take().is_some();
        dragged || pressed.is_some() || self.any_active(&hits)
    }

    /// `Activated`'s `clickCount`: measured 0, 1, 2 for quick repeats.
    fn click_count(&mut self, button: InstanceId) -> i32 {
        let now = self.runtime.world().scheduler.now();
        let count = match self.input.gui.last_activation {
            Some((b, at, n)) if b == button && now - at <= CLICK_RUN => n + 1,
            _ => 0,
        };
        self.input.gui.last_activation = Some((button, now, count));
        count
    }

    /// The wheel turned: `MouseWheelForward` or `Backward` and
    /// `InputChanged` on what's under the cursor, then the innermost
    /// ScrollingFrame that can move scrolls.
    pub(super) fn gui_wheel(&self, delta: f32, obj: Option<InstanceId>) -> bool {
        let hits = self.gui_hits();
        let at = self.window_point();
        let event = if delta > 0.0 {
            "MouseWheelForward"
        } else {
            "MouseWheelBackward"
        };
        for &id in &hits {
            self.fire(id, event, at);
            self.fire_object_input(id, "InputChanged", obj);
        }
        let scrolled = {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            w.gui.wheel(&w.dom, &hits, delta)
        };
        scrolled.is_some() || self.any_active(&hits)
    }

    /// Move the wheel's eased scrolls a frame on.
    pub(crate) fn ease_scrolls(&self) {
        let moves = {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            w.gui.ease_scrolls(&w.dom)
        };
        for (id, at) in moves {
            self.set_property(id, "CanvasPosition", Value::Vector2(Vector2(at)));
        }
    }
}

fn enum_value(enum_name: &str, item: &str) -> Option<Value> {
    let e = rook_reflection::database().enum_by_name(enum_name)?;
    rook_reflection::enum_item_value(e.id, item).map(Value::Enum)
}
