//! ProximityPrompts: which ones a client shows, their keys and holds, and
//! the events both sides fire (docs/behavior/gui/proximity-prompts.md).
//!
//! A client decides everything: [`update`] runs once a frame, after the
//! camera settles and before GUI is laid out, so the GUI a `PromptShown`
//! handler makes is drawn that frame. [`choose`] decides what shows;
//! [`hold`] owns holding and triggering, whether a key, `InputHoldBegin`
//! or hiding started or ended it. What fires on the client is queued for
//! the server, which fires it with the player.

mod choose;
mod hold;

use mlua::{Lua, Value as LuaValue};
use rook_dom::{Dom, InstanceId};
use rustc_hash::FxHashSet;

use super::RawMethod;
use crate::api::instance::push_instance;
use crate::ctx::with_world;
use crate::signal::SignalKey;
use crate::threads::fire;
use crate::types::enums::enum_item;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("ProximityPrompt", "InputHoldBegin") => hold::input_hold_begin,
        ("ProximityPrompt", "InputHoldEnd") => hold::input_hold_end,
        _ => return None,
    })
}

/// A client's prompts: what shows and what's held.
#[derive(Debug, Default)]
pub(crate) struct Prompts {
    /// Shown prompts, in the order they were shown: a key triggers them
    /// in this order (measured).
    shown: Vec<InstanceId>,
    /// Prompts showing their indicator.
    indicated: Vec<InstanceId>,
    holds: Vec<hold::Hold>,
    /// Keys whose press a prompt took, by `Enum.KeyCode` value: their
    /// release is processed too.
    keys: FxHashSet<u32>,
    /// What fired here, for the server.
    actions: Vec<Action>,
    /// The DOM indexes ProximityPrompts once the first frame asks.
    tracking: bool,
    /// Reused each frame.
    scratch: Vec<choose::Candidate>,
}

impl Prompts {
    /// The instances kept here, which must outlive their scripts' handles.
    pub(crate) fn held(&self) -> impl Iterator<Item = InstanceId> + '_ {
        let holds = self.holds.iter().map(|h| h.prompt);
        let actions = self.actions.iter().map(|a| a.prompt);
        (self.shown.iter().chain(&self.indicated).copied())
            .chain(holds)
            .chain(actions)
    }

    /// Drop what is kept about `id`, which is being freed.
    pub(crate) fn forget(&mut self, id: InstanceId) {
        self.shown.retain(|&p| p != id);
        self.indicated.retain(|&p| p != id);
        self.holds.retain(|h| h.prompt != id);
        self.actions.retain(|a| a.prompt != id);
    }
}

/// A prompt event that fired on a client, for the server to fire with the
/// player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Action {
    pub prompt: InstanceId,
    pub kind: ActionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    HoldBegan,
    HoldEnded,
    Triggered,
    TriggerEnded,
}

impl ActionKind {
    /// The prompt's event, then ProximityPromptService's.
    fn events(self) -> (&'static str, &'static str) {
        match self {
            Self::HoldBegan => ("PromptButtonHoldBegan", "PromptButtonHoldBegan"),
            Self::HoldEnded => ("PromptButtonHoldEnded", "PromptButtonHoldEnded"),
            Self::Triggered => ("Triggered", "PromptTriggered"),
            Self::TriggerEnded => ("TriggerEnded", "PromptTriggerEnded"),
        }
    }
}

fn prompt_class() -> rook_reflection::ClassId {
    static CLASS: std::sync::LazyLock<rook_reflection::ClassId> = std::sync::LazyLock::new(|| {
        (rook_reflection::database().class_by_name("ProximityPrompt"))
            .expect("ProximityPrompt")
            .id
    });
    *CLASS
}

fn service(dom: &Dom) -> Option<InstanceId> {
    dom.find_service("ProximityPromptService")
}

/// A client's frame: hide and show prompts and indicators as the camera
/// and character now see them, then let holds that lasted long enough
/// trigger.
pub(crate) fn update(lua: &Lua) {
    let Some((shown, indicated)) = with_world(lua, |w| {
        if !w.is_client {
            return None;
        }
        if !w.prompts.tracking {
            w.dom.track_class(prompt_class());
            w.prompts.tracking = true;
        }
        Some(choose::visible(w))
    }) else {
        return;
    };
    let (was_shown, was_indicated) = with_world(lua, |w| {
        (w.prompts.shown.clone(), w.prompts.indicated.clone())
    });
    for p in was_shown.iter().filter(|p| !shown.contains(p)) {
        hide(lua, *p);
    }
    for p in was_indicated.iter().filter(|p| !indicated.contains(p)) {
        unindicate(lua, *p);
    }
    for p in shown.iter().filter(|p| !was_shown.contains(p)) {
        show(lua, *p);
    }
    for p in indicated.iter().filter(|p| !was_indicated.contains(p)) {
        indicate(lua, *p);
    }
    hold::advance(lua);
}

/// The input type prompts show for. TODO: gamepad and touch, with the
/// platforms Rook gains them on (M19); until then always the keyboard.
fn input_type(lua: &Lua) -> mlua::Result<LuaValue> {
    let value = rook_reflection::database()
        .enum_by_name("ProximityPromptInputType")
        .and_then(|e| rook_reflection::enum_item_value(e.id, "Keyboard"))
        .unwrap_or(0);
    enum_item(lua, "ProximityPromptInputType", value)
}

/// Fire `event` on ProximityPromptService with the prompt first, then on
/// the prompt (measured: the service's handler ran first).
fn fire_shown_event(lua: &Lua, prompt: InstanceId, event: &'static str, args: &[LuaValue]) {
    let service = with_world(lua, |w| service(&w.dom));
    if let (Some(s), Ok(p)) = (service, push_instance(lua, prompt)) {
        let mut with_prompt = vec![LuaValue::UserData(p)];
        with_prompt.extend_from_slice(args);
        fire(lua, &SignalKey::event(s, event), &with_prompt);
    }
    fire(lua, &SignalKey::event(prompt, event), args);
}

fn show(lua: &Lua, prompt: InstanceId) {
    with_world(lua, |w| w.prompts.shown.push(prompt));
    let args = input_type(lua).map(|t| vec![t]).unwrap_or_default();
    fire_shown_event(lua, prompt, "PromptShown", &args);
}

/// Hide a shown prompt, ending its hold first (Inference: the order isn't
/// measured).
fn hide(lua: &Lua, prompt: InstanceId) {
    hold::release(lua, prompt);
    with_world(lua, |w| w.prompts.shown.retain(|&p| p != prompt));
    fire_shown_event(lua, prompt, "PromptHidden", &[]);
}

fn indicate(lua: &Lua, prompt: InstanceId) {
    with_world(lua, |w| w.prompts.indicated.push(prompt));
    fire_shown_event(lua, prompt, "IndicatorShown", &[]);
}

fn unindicate(lua: &Lua, prompt: InstanceId) {
    with_world(lua, |w| w.prompts.indicated.retain(|&p| p != prompt));
    fire_shown_event(lua, prompt, "IndicatorHidden", &[]);
}

/// `root` is being destroyed (`destroying`) or just moved: shown prompts
/// it takes out of Workspace hide now, while their handlers are still
/// connected (measured: a prompt's own `PromptHidden` fires when its part
/// is destroyed). Replicated removals wait for the next [`update`].
pub(crate) fn leaving(lua: &Lua, root: InstanceId, destroying: bool) {
    let gone = |w: &crate::world::World, list: &[InstanceId]| -> Vec<InstanceId> {
        let dom = &w.dom;
        (list.iter().copied())
            .filter(|&p| p == root || dom.is_ancestor_of(root, p))
            .filter(|&p| destroying || !choose::in_workspace(dom, p))
            .collect()
    };
    let (shown, indicated) = with_world(lua, |w| {
        if w.prompts.shown.is_empty() && w.prompts.indicated.is_empty() {
            return (Vec::new(), Vec::new());
        }
        (gone(w, &w.prompts.shown), gone(w, &w.prompts.indicated))
    });
    for p in shown {
        hide(lua, p);
    }
    for p in indicated {
        unindicate(lua, p);
    }
}

/// A key went down or up on a client, after a focused TextBox had its
/// chance. A press triggers or begins holding every shown prompt on that
/// key, in the order they were shown; its release ends those holds.
/// Returns whether prompts took it (its `gameProcessedEvent`).
pub(crate) fn key(lua: &Lua, key: u32, down: bool) -> bool {
    if !down {
        let taken = with_world(lua, |w| w.prompts.keys.remove(&key));
        if taken {
            hold::release_key(lua, key);
        }
        return taken;
    }
    let prompts = with_world(lua, |w| {
        let dom = &w.dom;
        (w.prompts.shown.iter().copied())
            .filter(|&p| dom.enum_property(p, "KeyboardKeyCode") == Some(key))
            .collect::<Vec<_>>()
    });
    if prompts.is_empty() {
        return false;
    }
    with_world(lua, |w| w.prompts.keys.insert(key));
    for p in prompts {
        hold::begin(lua, p, Some(key));
    }
    true
}

/// What fired here since the last call, for the server.
pub(crate) fn take_actions(lua: &Lua) -> Vec<Action> {
    with_world(lua, |w| std::mem::take(&mut w.prompts.actions))
}

/// A client's prompt event on the server: the prompt's event, then the
/// service's, with the player who sent it (measured order).
pub(crate) fn deliver(lua: &Lua, player: InstanceId, action: Action) {
    let ok = with_world(lua, |w| w.dom.is_a(action.prompt, "ProximityPrompt"));
    if ok {
        fire_action(lua, action, player);
    }
}

/// Fire `action`'s events with `player`.
fn fire_action(lua: &Lua, action: Action, player: InstanceId) {
    let Ok(player) = push_instance(lua, player) else {
        return;
    };
    let (event, service_event) = action.kind.events();
    let player = LuaValue::UserData(player);
    fire(
        lua,
        &SignalKey::event(action.prompt, event),
        std::slice::from_ref(&player),
    );
    let service = with_world(lua, |w| service(&w.dom));
    if let (Some(s), Ok(p)) = (service, push_instance(lua, action.prompt)) {
        fire(
            lua,
            &SignalKey::event(s, service_event),
            &[LuaValue::UserData(p), player],
        );
    }
}
