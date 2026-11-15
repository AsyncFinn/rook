//! Input that reaches scripts: InputObjects, PointerAction,
//! ContextActionService's bindings and TextBox focus.

use rook_dom::{Dom, InstanceId};
use rook_types::{UserInputState, UserInputType};

use crate::Runtime;

impl Runtime {
    /// Fire `UserInputService.PointerAction(wheel, pan, pinch,
    /// gameProcessedEvent)`: what the default camera zooms by.
    pub fn pointer_action(&self, wheel: f32, pan: (f32, f32), pinch: f32, processed: bool) {
        let Some(uis) = self.world().dom.find_service("UserInputService") else {
            return;
        };
        let pan = crate::types::LVector2(rook_types::Vector2::new(pan.0, pan.1));
        self.fire_event(uis, "PointerAction", (wheel, pan, pinch, processed));
    }

    /// Offer an InputObject GUI didn't take to ContextActionService's
    /// bindings. Returns whether one took it.
    pub fn context_action(&self, object: InstanceId) -> bool {
        let Ok(offer) = self
            .lua
            .named_registry_value::<mlua::Function>("rook.context_action")
        else {
            return false;
        };
        self.guarded(|| {
            let taken = crate::api::instance::push_instance(&self.lua, object)
                .and_then(|obj| offer.call::<bool>(obj))
                .unwrap_or_else(|e| {
                    tracing::warn!("ContextActionService: {e}");
                    false
                });
            crate::runtime::threads::drain_deferred(&self.lua);
            taken
        })
    }

    /// Give TextBox `id` focus as clicking it does, its caret before
    /// character `caret` (`None`: at the end), firing what that fires.
    pub fn focus_text_box(&self, id: InstanceId, caret: Option<usize>) {
        let focused =
            self.guarded(|| crate::api::methods::gui::textbox::focus(&self.lua, id, caret));
        if let Err(e) = focused {
            tracing::warn!("focusing a TextBox failed: {e}");
        }
    }

    /// Take focus from the focused TextBox, as a key or a click elsewhere
    /// does: `FocusLost(enter_pressed, cause)`, then
    /// `TextBoxFocusReleased`.
    pub fn release_text_box(&self, enter_pressed: bool, cause: Option<InstanceId>) {
        let released = self.guarded(|| {
            let cause = match cause {
                Some(id) => {
                    mlua::Value::UserData(crate::api::instance::push_instance(&self.lua, id)?)
                }
                None => mlua::Value::Nil,
            };
            crate::api::methods::gui::textbox::release(&self.lua, enter_pressed, cause, false)
        });
        if let Err(e) = released {
            tracing::warn!("releasing a TextBox failed: {e}");
        }
    }

    pub fn focused_text_box(&self) -> Option<InstanceId> {
        crate::api::methods::gui::textbox::focused(&self.lua)
    }
}

/// Create an `InputObject` describing one input (an `Enum.KeyCode` value;
/// position and delta in pixels). Used by the engine when firing input events and by
/// UserInputService queries.
pub fn input_object(
    dom: &mut Dom,
    key_code: u32,
    input_type: UserInputType,
    state: UserInputState,
    position: (f32, f32),
    delta: (f32, f32),
) -> Option<InstanceId> {
    use rook_types::{Value, Vector3};
    let id = dom.create_by_name("InputObject").ok()?;
    let class = dom.get(id)?.class;
    let db = dom.database();
    let set = |dom: &mut Dom, name: &str, value: Value| {
        if let Some(p) = db.find_property(class, name) {
            dom.load_property(id, p.id, value);
        }
    };
    set(dom, "KeyCode", Value::Enum(key_code));
    set(dom, "UserInputType", input_type.into());
    set(dom, "UserInputState", state.into());
    set(
        dom,
        "Position",
        Value::Vector3(Vector3::new(position.0, position.1, 0.0)),
    );
    set(
        dom,
        "Delta",
        Value::Vector3(Vector3::new(delta.0, delta.1, 0.0)),
    );
    Some(id)
}
