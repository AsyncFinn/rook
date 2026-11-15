//! Picking Tools up by touch, on the server (measured in Studio 0.747,
//! docs/behavior/characters/tools.md#picking-up): a Tool anywhere in
//! Workspace, outside a character, whose Handle a player's character
//! touches is equipped by it. An NPC's touch picks nothing up.

use mlua::Lua;
use rook_dom::{Dom, InstanceId};

use super::{handle, is_character};
use crate::ctx::with_world;

/// The character, living, whose part `part` is, and its player's Backpack.
fn toucher(dom: &Dom, part: InstanceId) -> Option<(InstanceId, Option<InstanceId>)> {
    let players = dom.find_service("Players")?;
    let character = dom.ancestors(part).find(|a| is_character(dom, *a))?;
    let player = dom
        .children(players)
        .iter()
        .copied()
        .find(|p| dom.ref_property(*p, "Character") == Some(character))?;
    // Inference: a dead character picks nothing up.
    let humanoid = dom.find_first_child_of_class(character, "Humanoid")?;
    if dom
        .number_property(humanoid, "Health_XML")
        .is_some_and(|h| h <= 0.0)
    {
        return None;
    }
    Some((character, dom.find_first_child_of_class(player, "Backpack")))
}

/// The Tool `part` is the Handle of, when it lies in Workspace outside a
/// character.
fn lying_tool(dom: &Dom, part: InstanceId) -> Option<InstanceId> {
    let tool = dom.parent(part).filter(|t| dom.is_a(*t, "Tool"))?;
    if handle(dom, tool) != Some(part) {
        return None;
    }
    let workspace = dom.find_service("Workspace")?;
    let mut above = dom.ancestors(tool);
    let in_world = above.any(|a| a == workspace);
    let carried = dom.ancestors(tool).any(|a| is_character(dom, a));
    (in_world && !carried).then_some(tool)
}

/// `part` began touching `other`, on the server.
pub(crate) fn touched(lua: &Lua, part: InstanceId, other: InstanceId) {
    let found = with_world(lua, |w| {
        let dom = &w.dom;
        Some((lying_tool(dom, part)?, toucher(dom, other)?))
    });
    let Some((tool, (character, backpack))) = found else {
        return;
    };
    if let Err(e) = super::moves::equip(lua, character, backpack, tool) {
        tracing::debug!("picking up a tool failed: {e:?}");
    }
}
