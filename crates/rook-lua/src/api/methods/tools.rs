//! Tools: held in a character's right hand, used with the mouse, dropped
//! and picked up (docs/behavior/characters/tools.md).
//!
//! [`holder`] is the one rule for whether a Tool is held: `Equipped`,
//! activation, the grip and the Tool's Mouse all ask it. Moves made from
//! Luau go through `removal::set_parent`, which runs [`moves::before`] and
//! [`moves::after`] around every reparent, so a Tool leaving or reaching a
//! hand by any route (a Parent write, `EquipTool`, a drop, a pickup) gets
//! its grip made or taken away. Moves the client copies from the server
//! don't: the server's grip comes with them.

mod activation;
mod grip;
mod mouse;
mod moves;
mod pickup;

use mlua::Lua;
use rook_dom::{Dom, InstanceId};

pub use activation::Activation;
pub(crate) use activation::{activate_held, deliver_activation};
pub(crate) use grip::follow_grip;
pub(crate) use mouse::{equipped_mouse, inactive_mouse_error, is_tool_mouse, retire_mice};
pub use moves::ToolMove;
pub(crate) use moves::{after, before, drop_held, repeat_move};
pub(crate) use pickup::touched;

use super::RawMethod;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Humanoid", "EquipTool") => moves::equip_tool,
        ("Humanoid", "UnequipTools") => moves::unequip_tools,
        ("Tool", "Activate") => activation::activate,
        ("Tool", "Deactivate") => activation::deactivate,
        ("HopperBin", "ToggleSelect") => toggle_select,
        _ => return None,
    })
}

/// Whether `model` is a character: it holds a Humanoid.
fn is_character(dom: &Dom, model: InstanceId) -> bool {
    dom.children(model).iter().any(|c| dom.is_a(*c, "Humanoid"))
}

/// The Tool's `Handle`: a part by that name directly inside it.
pub(crate) fn handle(dom: &Dom, tool: InstanceId) -> Option<InstanceId> {
    dom.find_first_child(tool, "Handle", false)
        .filter(|h| dom.is_a(*h, "BasePart"))
}

/// The character that would hold `tool` under `parent`: `parent` when it's
/// a character and the Tool has a Handle or doesn't need one (measured: a
/// Tool without one whose `RequiresHandle` is on sits in the character
/// unheld, firing no `Equipped`).
pub(crate) fn holder_at(dom: &Dom, tool: InstanceId, parent: InstanceId) -> Option<InstanceId> {
    if !dom.is_a(tool, "Tool") || !is_character(dom, parent) {
        return None;
    }
    let held =
        handle(dom, tool).is_some() || dom.bool_property(tool, "RequiresHandle") == Some(false);
    held.then_some(parent)
}

/// The character holding `tool`, if one is.
pub(crate) fn holder(dom: &Dom, tool: InstanceId) -> Option<InstanceId> {
    holder_at(dom, tool, dom.parent(tool)?)
}

/// The Tools `character` holds, in child order.
fn held_tools(dom: &Dom, character: InstanceId) -> Vec<InstanceId> {
    dom.children(character)
        .iter()
        .copied()
        .filter(|t| holder_at(dom, *t, character).is_some())
        .collect()
}

/// `HopperBin:ToggleSelect()`, which the backpack CoreScript calls: it
/// only flips `Active` (measured: modern Roblox fires no `Selected` and
/// equips nothing).
fn toggle_select(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    super::mutate(lua, |w| {
        let active = w.dom.bool_property(this, "Active").unwrap_or(false);
        w.dom
            .set_property_by_name(this, "Active", rook_types::Value::Bool(!active))
    })?;
    Outcome::ret(lua, ())
}

/// The local player's character on a client.
fn local_character(lua: &Lua) -> Option<InstanceId> {
    with_world(lua, |w| {
        let player = w.dom.local_player()?;
        w.dom.ref_property(player, "Character")
    })
}
