//! Input events delivered to the DataModel: mouse moves, buttons and the
//! wheel, routed to the GUI, ClickDetectors and the mouse objects.

use super::*;

impl DataModel {
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

    pub(crate) fn ensure_mouse(&self) {
        let mut w = self.runtime.world_mut();
        if w.input.mouse_object.is_none()
            && let Ok(m) = w.dom.create_by_name("PlayerMouse")
        {
            w.input.mouse_object = Some(m);
        }
    }

    /// Bindings hear input before UserInputService, and one that sinks it
    /// makes the event processed (measured,
    /// docs/behavior/input/context-actions.md). Input GUI took isn't
    /// offered (Inference).
    pub(super) fn deliver(&self, event: &'static str, object: InstanceId, gui_took: bool) -> bool {
        let processed = gui_took || self.runtime.context_action(object);
        let uis = self
            .runtime
            .world_mut()
            .dom
            .get_service("UserInputService")
            .ok();
        if let (Some(uis), Ok(obj)) = (uis, self.runtime.push_instance(object)) {
            self.fire(uis, event, (obj, processed));
        }
        processed
    }

    /// Look under the mouse again after a frame; the ClickDetector hovers
    /// it changed.
    pub fn refresh_hover(&mut self) -> Vec<ClickEvent> {
        let mut clicks = Vec::new();
        self.refresh_mouse(&mut clicks);
        clicks
    }

    pub(super) fn mouse_object(&self) -> Option<InstanceId> {
        self.runtime.world().input.mouse_object
    }

    /// The Mice that hear the mouse: those `Equipped` passed for the
    /// Tools the local character holds, then the PlayerMouse (measured:
    /// a tool Mouse's `Button1Down` fires first).
    pub(super) fn mice(&self) -> Vec<InstanceId> {
        let w = self.runtime.world();
        let tools = w.input.tool_mice.iter().map(|&(_, m)| m);
        tools.chain(w.input.mouse_object).collect()
    }

    /// Fire `event` on every Mouse.
    pub(super) fn fire_mice(&self, event: &'static str) {
        for m in self.mice() {
            self.fire(m, event, ());
        }
    }

    /// Handle one input event. Returns the ClickDetector interactions it
    /// caused, for the session to fire on both sides.
    pub fn input(&mut self, event: InputEvent) -> Vec<ClickEvent> {
        profiling::scope!("DataModel::input");
        self.ensure_mouse();
        let mut clicks = Vec::new();
        self.input.processed = match event {
            InputEvent::Focus { focused } => self.focus(focused),
            InputEvent::GamepadConnected {
                pad,
                kind,
                supported,
            } => self.gamepad_connected(pad, kind, supported),
            InputEvent::GamepadDisconnected { pad } => self.gamepad_disconnected(pad),
            InputEvent::GamepadButton { pad, key, down } => self.gamepad_button(pad, key, down),
            InputEvent::GamepadAxis { pad, axis, value } => self.gamepad_axis(pad, axis, value),
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
    pub(super) fn mouse_move(&mut self, x: f32, y: f32, clicks: &mut Vec<ClickEvent>) -> bool {
        self.note_input_type(UserInputType::MouseMovement, None);
        {
            let mut w = self.runtime.world_mut();
            w.input.mouse = (x, y);
            w.input.mouse_delta = (0.0, 0.0);
        }
        let (gx, gy) = gui_point((x, y));
        let obj =
            self.changed_input_object(UserInputType::MouseMovement, (gx, gy, 0.0), (0.0, 0.0));
        let mut processed = self.gui_move(obj);
        if let Some(obj) = obj {
            processed = self.deliver("InputChanged", obj, processed);
        }
        self.refresh_mouse(clicks);
        self.fire_mice("Move");
        processed
    }

    /// Movement of a held mouse: InputChanged with the movement in
    /// `Delta`, and the frame's movement so far in `GetMouseDelta` (summed
    /// over the frame's motion events, zeroed when the frame ends), the
    /// cursor at the window's center for
    /// [`MouseLock::Center`](crate::MouseLock) (Inference: where Roblox
    /// draws it in first person), else where it was.
    pub(super) fn mouse_motion(&mut self, dx: f32, dy: f32) -> bool {
        self.note_input_type(UserInputType::MouseMovement, None);
        self.hold_mouse();
        let pos = {
            let mut w = self.runtime.world_mut();
            w.input.mouse_delta.0 += dx;
            w.input.mouse_delta.1 += dy;
            w.input.mouse
        };
        let (gx, gy) = gui_point(pos);
        let obj = self.changed_input_object(UserInputType::MouseMovement, (gx, gy, 0.0), (dx, dy));
        obj.is_some_and(|obj| self.deliver("InputChanged", obj, false))
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

    pub(super) fn wheel(&mut self, delta: f32) -> bool {
        self.note_input_type(UserInputType::MouseWheel, None);
        let pos = gui_point(self.runtime.world().input.mouse);
        // The wheel amount is in Position.Z (creator-docs InputObject).
        let obj =
            self.changed_input_object(UserInputType::MouseWheel, (pos.0, pos.1, delta), (0.0, 0.0));
        let mut processed = self.gui_wheel(delta, obj);
        if let Some(obj) = obj {
            processed = self.deliver("InputChanged", obj, processed);
        }
        // Inference: after InputChanged, as Roblox's camera reads both.
        self.runtime
            .pointer_action(delta, (0.0, 0.0), 0.0, processed);
        self.fire_mice(if delta > 0.0 {
            "WheelForward"
        } else {
            "WheelBackward"
        });
        processed
    }

    pub(super) fn mouse_button(
        &mut self,
        button: u8,
        down: bool,
        clicks: &mut Vec<ClickEvent>,
    ) -> bool {
        if !(1..=3).contains(&button) {
            return false;
        }
        let index = usize::from(button - 1);
        self.note_input_type(
            UserInputType::MOUSE_BUTTONS[index],
            Some(Preferred::KeyboardAndMouse),
        );
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
        let gui_took = match (button, down) {
            (3, _) => self.gui_covers_active(),
            (_, true) => self.gui_press(button, obj),
            (_, false) => self.gui_release(button, obj),
        };
        let event = if down { "InputBegan" } else { "InputEnded" };
        let processed = obj.map_or(gui_took, |obj| self.deliver(event, obj, gui_took));
        let detector = self.refresh_mouse(clicks);
        if button <= 2 {
            self.fire_mice(match (button, down) {
                (1, true) => "Button1Down",
                (1, false) => "Button1Up",
                (_, true) => "Button2Down",
                (_, false) => "Button2Up",
            });
        }
        // Measured: a left click GUI didn't take activates the Tools the
        // character holds; a right click nothing. After the Mouse's events
        // (Inference).
        if button == 1 && !processed {
            self.runtime.activate_tools(down);
        }
        // A click is a press and release over the same detector
        // (creator-docs ClickDetector.MouseClick); measured, an Active
        // GUI object over it takes the click.
        if button <= 2 {
            let slot = &mut self.input.pressed[index];
            if down {
                *slot = detector.filter(|_| !gui_took);
            } else if let (Some(pressed), Some(now)) = (slot.take(), detector)
                && pressed == now
                && !gui_took
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
