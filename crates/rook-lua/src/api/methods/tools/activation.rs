//! `Activated` and `Deactivated`: from left clicks while a Tool is held,
//! and from `Tool:Activate()` and `Deactivate()` (measured in Studio
//! 0.747, docs/behavior/characters/tools.md#activating). Each fires on the
//! side where it began and then on the other, so every activation is
//! queued for the session to carry across.

use mlua::Lua;
use rook_dom::InstanceId;

use super::{held_tools, holder};
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};
use crate::signal::SignalKey;
use crate::threads::fire;

/// An activation to fire on the other side: the Tool, and whether it was
/// `Activated` (else `Deactivated`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Activation {
    pub tool: InstanceId,
    pub activated: bool,
}

fn event(activated: bool) -> &'static str {
    if activated {
        "Activated"
    } else {
        "Deactivated"
    }
}

/// Fire the activation here and queue it for the other side. Nothing
/// fires for a disabled Tool (measured: no warning either).
fn begin(lua: &Lua, tool: InstanceId, activated: bool) {
    let enabled = with_world(lua, |w| w.dom.bool_property(tool, "Enabled") != Some(false));
    if !enabled {
        return;
    }
    fire(lua, &SignalKey::event(tool, event(activated)), &[]);
    with_world(lua, |w| {
        w.tool_activations.push(Activation { tool, activated })
    });
}

/// The left button went down (`activated`) or up over the world, on a
/// client: the Tools its character holds activate. A press skips Tools
/// with `ManualActivationOnly`; the release doesn't (measured).
pub(crate) fn activate_held(lua: &Lua, activated: bool) {
    let Some(character) = super::local_character(lua) else {
        return;
    };
    let tools = with_world(lua, |w| {
        let dom = &w.dom;
        held_tools(dom, character)
            .into_iter()
            .filter(|t| !activated || dom.bool_property(*t, "ManualActivationOnly") != Some(true))
            .collect::<Vec<_>>()
    });
    for tool in tools {
        begin(lua, tool, activated);
    }
}

/// An activation from the other side. On the server, only of a Tool the
/// sending player's character holds.
pub(crate) fn deliver_activation(lua: &Lua, player: Option<InstanceId>, a: Activation) {
    let allowed = with_world(lua, |w| {
        let dom = &w.dom;
        let held_by = holder(dom, a.tool);
        let mine =
            player.is_none_or(|p| held_by.is_some() && held_by == dom.ref_property(p, "Character"));
        held_by.is_some() && mine && dom.bool_property(a.tool, "Enabled") != Some(false)
    });
    if allowed {
        fire(lua, &SignalKey::event(a.tool, event(a.activated)), &[]);
    }
}

/// `Tool:Activate()`. On a Tool no one holds it warns and fires nothing
/// (measured, with Studio's words).
pub(super) fn activate(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    if with_world(lua, |w| holder(&w.dom, this)).is_none() {
        with_world(lua, |w| {
            w.push_log(
                crate::MessageType::Warning,
                "Tool:Activate() called from script when tool is not equipped. Tool will not be activated."
                    .to_owned(),
            );
        });
    } else {
        begin(lua, this, true);
    }
    Outcome::ret(lua, ())
}

/// `Tool:Deactivate()`. On a Tool no one holds it does nothing
/// (Inference: Studio's warning was only seen from `Activate`).
pub(super) fn deactivate(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    if with_world(lua, |w| holder(&w.dom, this)).is_some() {
        begin(lua, this, false);
    }
    Outcome::ret(lua, ())
}
