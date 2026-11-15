//! Keys pressed and released: KeyCode values, the legacy key strings
//! and the InputObjects each key's input fires with.

use super::*;

/// What `Mouse.KeyDown` passes for a key: the character it types, for the
/// keys that type one (creator-docs Mouse.KeyDown, deprecated).
pub(super) fn legacy_key_string(code: &str) -> Option<String> {
    pub(super) const DIGITS: [&str; 10] = [
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

pub(super) fn key_code_value(code: &str) -> Option<u32> {
    let e = rook_reflection::database().enum_by_name("KeyCode")?;
    rook_reflection::enum_item_value(e.id, code)
}

impl DataModel {
    pub(super) fn new_input_object(
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
    pub(super) fn changed_input_object(
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
    pub(super) fn key(&mut self, code: &str, down: bool) -> bool {
        let Some(value) = key_code_value(code) else {
            return false;
        };
        let pos = gui_point(self.runtime.world().input.mouse);
        let mut processed = self.focused_text_box().is_some();
        // Inference: a binding sinking a key doesn't hide it from the
        // legacy Mice, which hear what GUI, TextBoxes and prompts don't
        // take.
        let mouse_hears;
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
            } else {
                // Measured: shown prompts on the key trigger before
                // UserInputService hears it, as processed.
                processed = self.runtime.prompt_key(value, true);
            }
            self.note_input_type(UserInputType::Keyboard, Some(Preferred::KeyboardAndMouse));
            mouse_hears = !processed;
            processed = self.deliver("InputBegan", obj, processed);
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
            // A release whose press a prompt took lets go of it, processed
            // (measured).
            processed |= self.runtime.prompt_key(value, false);
            self.set_property(obj, "UserInputState", UserInputState::End.into());
            self.note_input_type(UserInputType::Keyboard, Some(Preferred::KeyboardAndMouse));
            mouse_hears = !processed;
            processed = self.deliver("InputEnded", obj, processed);
        }
        if mouse_hears && let Some(s) = legacy_key_string(code) {
            for m in self.mice() {
                self.fire(m, if down { "KeyDown" } else { "KeyUp" }, s.clone());
            }
        }
        // Measured: the engine drops the held Tool, not the backpack
        // CoreScript. On the press (Inference).
        if down && !processed && code == "Backspace" {
            self.runtime.drop_tools();
        }
        processed
    }
}
