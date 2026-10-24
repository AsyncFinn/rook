//! Tools between a player's Backpack and character. A Tool moved into a
//! character fires `Equipped`, one moved out `Unequipped` (`signal::expand`).
//!
//! A client may move its own player's Tools between the two, and the
//! server repeats the move (Inference: Roblox's backpack CoreScript equips
//! with `tool.Parent = Character` on the client, and a server Script in
//! the tool hears `Equipped`); any other change a client makes stays on
//! the client.

use mlua::Lua;
use rook_dom::{Dom, InstanceId};

use super::RawMethod;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::world::World;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Humanoid", "EquipTool") => equip_tool,
        ("Humanoid", "UnequipTools") => unequip_tools,
        _ => return None,
    })
}

/// The character a Humanoid belongs to, and its player's Backpack.
fn owner(dom: &Dom, humanoid: InstanceId) -> (Option<InstanceId>, Option<InstanceId>) {
    let character = dom.parent(humanoid);
    let backpack = character.and_then(|c| {
        let players = dom.find_service("Players")?;
        let player = dom
            .children(players)
            .iter()
            .copied()
            .find(|p| dom.ref_property(*p, "Character") == Some(c))?;
        dom.find_first_child_of_class(player, "Backpack")
    });
    (character, backpack)
}

fn tools_in(dom: &Dom, character: InstanceId) -> Vec<InstanceId> {
    dom.children(character)
        .iter()
        .copied()
        .filter(|c| dom.is_a(*c, "Tool"))
        .collect()
}

/// Move `tool` under `parent` with its signals, and on a client note the
/// moves the server repeats.
fn move_tool(lua: &Lua, tool: InstanceId, parent: InstanceId) -> ApiResult<()> {
    crate::removal::set_parent(lua, tool, Some(parent)).map_err(ScriptError::from)?;
    with_world(lua, |w| note_move(w, tool, Some(parent)));
    Ok(())
}

/// `Humanoid:UnequipTools()`: the character's Tools go to its player's
/// Backpack (creator-docs Humanoid). Without a player they stay
/// (Inference).
fn unequip_tools(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let (character, backpack) = with_world(lua, |w| owner(&w.dom, this));
    if let (Some(character), Some(backpack)) = (character, backpack) {
        for tool in with_world(lua, |w| tools_in(&w.dom, character)) {
            move_tool(lua, tool, backpack)?;
        }
    }
    Outcome::ret(lua, ())
}

/// `Humanoid:EquipTool(tool)`: the Tools held go back to the Backpack, and
/// `tool` moves into the character (creator-docs Humanoid).
fn equip_tool(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let tool = a.instance(0)?;
    if !with_world(lua, |w| w.dom.is_a(tool, "Tool")) {
        return Outcome::ret(lua, ());
    }
    let (character, backpack) = with_world(lua, |w| owner(&w.dom, this));
    let Some(character) = character else {
        return Outcome::ret(lua, ());
    };
    if let Some(backpack) = backpack {
        for held in with_world(lua, |w| tools_in(&w.dom, character)) {
            if held != tool {
                move_tool(lua, held, backpack)?;
            }
        }
    }
    move_tool(lua, tool, character)?;
    Outcome::ret(lua, ())
}

/// On the server, repeat a move `player`'s client made: only of a Tool in
/// that player's Backpack or character, to the other one.
pub(crate) fn repeat_move(lua: &Lua, player: InstanceId, tool: InstanceId, parent: InstanceId) {
    let allowed = with_world(lua, |w| {
        let dom = &w.dom;
        let mine = |at: Option<InstanceId>| {
            at.is_some_and(|at| {
                dom.ref_property(player, "Character") == Some(at)
                    || dom.find_first_child_of_class(player, "Backpack") == Some(at)
            })
        };
        dom.is_a(tool, "Tool") && mine(dom.parent(tool)) && mine(Some(parent))
    });
    if allowed {
        let _ = crate::removal::set_parent(lua, tool, Some(parent));
    }
}

/// On a client, a Tool now under the local player's Backpack or character
/// is queued for the server to move too.
pub(crate) fn note_move(w: &mut World, tool: InstanceId, parent: Option<InstanceId>) {
    if !w.is_client || !w.dom.is_a(tool, "Tool") {
        return;
    }
    let Some((parent, player)) = parent.zip(w.dom.local_player()) else {
        return;
    };
    let own = w.dom.ref_property(player, "Character") == Some(parent)
        || (w.dom.parent(parent) == Some(player) && w.dom.is_a(parent, "Backpack"));
    if own {
        w.tool_moves.push((tool, parent));
    }
}
