//! Holding and triggering a shown prompt (measured in Studio 0.744,
//! docs/behavior/gui/proximity-prompts.md#holding): with no
//! `HoldDuration` a press triggers at once; with one, it fires
//! `PromptButtonHoldBegan`, and the first frame the hold has lasted that
//! long, `PromptButtonHoldEnded` then `Triggered`. Letting go fires
//! `TriggerEnded` after a trigger, `PromptButtonHoldEnded` before one;
//! hiding the prompt lets go too.

use mlua::Lua;
use rook_dom::InstanceId;

use super::{Action, ActionKind};
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};

#[derive(Debug, Clone, Copy)]
pub(super) struct Hold {
    pub(super) prompt: InstanceId,
    /// When it began, on the DataModel's clock.
    began: f64,
    triggered: bool,
    /// The key holding it, by `Enum.KeyCode` value (`None`:
    /// `InputHoldBegin`).
    key: Option<u32>,
}

/// Begin holding `prompt` if it shows and isn't held already (measured: a
/// second `InputHoldBegin` does nothing, and the first `InputHoldEnd`
/// lets go).
pub(super) fn begin(lua: &Lua, prompt: InstanceId, key: Option<u32>) {
    let instant = with_world(lua, |w| {
        let p = &mut w.prompts;
        if !p.shown.contains(&prompt) || p.holds.iter().any(|h| h.prompt == prompt) {
            return None;
        }
        let instant = w.dom.float_property(prompt, "HoldDuration").unwrap_or(0.0) <= 0.0;
        p.holds.push(Hold {
            prompt,
            began: w.scheduler.now(),
            triggered: instant,
            key,
        });
        Some(instant)
    });
    match instant {
        Some(true) => emit(lua, prompt, ActionKind::Triggered),
        Some(false) => emit(lua, prompt, ActionKind::HoldBegan),
        None => {}
    }
}

/// Let go of `prompt`, if it's held.
pub(super) fn release(lua: &Lua, prompt: InstanceId) {
    let hold = with_world(lua, |w| {
        let holds = &mut w.prompts.holds;
        let i = holds.iter().position(|h| h.prompt == prompt)?;
        Some(holds.remove(i))
    });
    match hold {
        Some(h) if h.triggered => emit(lua, prompt, ActionKind::TriggerEnded),
        Some(_) => emit(lua, prompt, ActionKind::HoldEnded),
        None => {}
    }
}

/// `key` came up: let go of what it holds, in the order it took them.
pub(super) fn release_key(lua: &Lua, key: u32) {
    let held = with_world(lua, |w| {
        (w.prompts.holds.iter())
            .filter(|h| h.key == Some(key))
            .map(|h| h.prompt)
            .collect::<Vec<_>>()
    });
    for prompt in held {
        release(lua, prompt);
    }
}

/// Trigger the holds that have lasted their prompt's `HoldDuration`.
pub(super) fn advance(lua: &Lua) {
    let due = with_world(lua, |w| {
        let now = w.scheduler.now();
        let dom = &w.dom;
        let mut due = Vec::new();
        for h in w.prompts.holds.iter_mut().filter(|h| !h.triggered) {
            let duration = dom.float_property(h.prompt, "HoldDuration").unwrap_or(0.0);
            if now - h.began >= f64::from(duration) {
                h.triggered = true;
                due.push(h.prompt);
            }
        }
        due
    });
    for prompt in due {
        emit(lua, prompt, ActionKind::HoldEnded);
        emit(lua, prompt, ActionKind::Triggered);
    }
}

/// Fire `kind` here with the local player and queue it for the server.
fn emit(lua: &Lua, prompt: InstanceId, kind: ActionKind) {
    let action = Action { prompt, kind };
    let player = with_world(lua, |w| {
        w.prompts.actions.push(action);
        w.dom.local_player()
    });
    if let Some(player) = player {
        super::fire_action(lua, action, player);
    }
}

/// `ProximityPrompt:InputHoldBegin()`, which Roblox's prompt GUI calls
/// for clicks and touches: as the prompt's key going down.
pub(super) fn input_hold_begin(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    begin(lua, this, None);
    Outcome::ret(lua, ())
}

/// `ProximityPrompt:InputHoldEnd()`: lets go, however the hold began.
pub(super) fn input_hold_end(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    release(lua, this);
    Outcome::ret(lua, ())
}
