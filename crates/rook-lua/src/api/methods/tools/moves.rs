//! Tools moving between a player's Backpack, their character and the
//! world: `Humanoid:EquipTool` and `UnequipTools`, the grip made and
//! taken around every Luau reparent, dropping, and the moves a client
//! makes that the server repeats.
//!
//! A client may move its own player's Tools between their Backpack and
//! character (Roblox's backpack CoreScript equips with `tool.Parent =
//! Character`), and drop a held one into Workspace; the server repeats
//! each, a drop only when `CanBeDropped` is on (measured). Any other
//! change a client makes stays on the client.

use mlua::Lua;
use rook_dom::{Dom, InstanceId};

use super::{grip, held_tools, holder};
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::world::World;

/// A Tool move a client's scripts made, for the server to repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolMove {
    pub tool: InstanceId,
    pub parent: InstanceId,
    /// The grip weld the client made for its own hand, which stands for
    /// the one the server makes when it repeats the move.
    pub grip: Option<InstanceId>,
}

/// Where a Tool was before a Luau move: its parent and the character
/// holding it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Leaving {
    parent: Option<InstanceId>,
    held_by: Option<InstanceId>,
}

/// Before `id` moves under `parent`: a held Tool leaving its character lets
/// go of its grip first (measured: the grip goes and the Handle's
/// `CanCollide` changes before `Unequipped`). `None` for anything but a
/// Tool.
pub(crate) fn before(lua: &Lua, id: InstanceId, parent: Option<InstanceId>) -> Option<Leaving> {
    let leaving = with_world(lua, |w| {
        w.dom.is_a(id, "Tool").then(|| Leaving {
            parent: w.dom.parent(id),
            held_by: holder(&w.dom, id),
        })
    })?;
    if let Some(character) = leaving.held_by
        && parent != Some(character)
    {
        grip::release(lua, id, character);
    }
    Some(leaving)
}

/// After a Tool moved: one a character now holds takes its grip, after
/// `Equipped` (measured), and a client notes the moves of its own Tools
/// for the server.
pub(crate) fn after(lua: &Lua, tool: InstanceId, leaving: Leaving) {
    let held_by = with_world(lua, |w| holder(&w.dom, tool));
    if let Some(character) = held_by
        && leaving.held_by != Some(character)
    {
        grip::hold(lua, tool, character);
    }
    with_world(lua, |w| {
        // At once, as Studio does; moves copied from the server retire
        // theirs when dispatched.
        super::retire_mice(w, tool);
        note_move(w, tool, leaving.parent);
    });
}

/// A player's character and Backpack, by either.
struct Owned {
    character: Option<InstanceId>,
    backpack: Option<InstanceId>,
}

impl Owned {
    fn of_player(dom: &Dom, player: InstanceId) -> Self {
        Self {
            character: dom.ref_property(player, "Character"),
            backpack: dom.find_first_child_of_class(player, "Backpack"),
        }
    }

    fn holds(&self, at: Option<InstanceId>) -> bool {
        at.is_some() && (at == self.character || at == self.backpack)
    }
}

/// The player whose character holds `humanoid`.
fn player_of(dom: &Dom, character: InstanceId) -> Option<InstanceId> {
    let players = dom.find_service("Players")?;
    dom.children(players)
        .iter()
        .copied()
        .find(|p| dom.ref_property(*p, "Character") == Some(character))
}

/// On a client, note a move of the local player's own Tool for the server
/// to repeat: between its Backpack and character, or out of the character
/// into Workspace (a drop).
pub(crate) fn note_move(w: &mut World, tool: InstanceId, old: Option<InstanceId>) {
    if !w.is_client {
        return;
    }
    let dom = &w.dom;
    let (Some(player), Some(parent)) = (dom.local_player(), dom.parent(tool)) else {
        return;
    };
    let own = Owned::of_player(dom, player);
    let dropped =
        old.is_some() && old == own.character && Some(parent) == dom.find_service("Workspace");
    if !(own.holds(old) && own.holds(Some(parent)) || dropped) {
        return;
    }
    let grip = (Some(parent) == own.character)
        .then(|| grip::find(dom, tool))
        .flatten();
    w.tool_moves.push(ToolMove { tool, parent, grip });
}

/// On the server, repeat a move `player`'s client made. Returns the grip
/// the server made for it, which the client's own stands for.
pub(crate) fn repeat_move(
    lua: &Lua,
    player: InstanceId,
    tool: InstanceId,
    parent: InstanceId,
) -> Option<InstanceId> {
    let allowed = with_world(lua, |w| {
        let dom = &w.dom;
        let own = Owned::of_player(dom, player);
        let from = dom.parent(tool);
        let drop = from.is_some()
            && from == own.character
            && Some(parent) == dom.find_service("Workspace")
            && dom.bool_property(tool, "CanBeDropped") != Some(false);
        dom.is_a(tool, "Tool") && own.holds(from) && (own.holds(Some(parent)) || drop)
    });
    if !allowed {
        return None;
    }
    if let Err(e) = crate::removal::set_parent(lua, tool, Some(parent)) {
        tracing::debug!("a client's tool move failed on the server: {e}");
        return None;
    }
    with_world(lua, |w| grip::find(&w.dom, tool))
}

/// Move `tool` under `parent` as a script's Parent write does.
fn move_tool(lua: &Lua, tool: InstanceId, parent: Option<InstanceId>) -> ApiResult<()> {
    crate::removal::set_parent(lua, tool, parent).map_err(ScriptError::from)
}

/// `character` puts down the Tools it holds and takes `tool`, as
/// `Humanoid:EquipTool` does: those go to `backpack`, or to nil without
/// one (measured on an NPC). Touch pickup equips this way too.
pub(super) fn equip(
    lua: &Lua,
    character: InstanceId,
    backpack: Option<InstanceId>,
    tool: InstanceId,
) -> ApiResult<()> {
    for held in with_world(lua, |w| held_tools(&w.dom, character)) {
        if held != tool {
            move_tool(lua, held, backpack)?;
        }
    }
    move_tool(lua, tool, Some(character))
}

/// The character a Humanoid belongs to, and its player's Backpack.
fn owner(dom: &Dom, humanoid: InstanceId) -> (Option<InstanceId>, Option<InstanceId>) {
    let character = dom.parent(humanoid);
    let backpack = character
        .and_then(|c| player_of(dom, c))
        .and_then(|p| dom.find_first_child_of_class(p, "Backpack"));
    (character, backpack)
}

/// `Humanoid:UnequipTools()`: the Tools held go to the player's Backpack,
/// or to nil on a character without a player (measured).
pub(super) fn unequip_tools(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let (character, backpack) = with_world(lua, |w| owner(&w.dom, this));
    if let Some(character) = character {
        for tool in with_world(lua, |w| held_tools(&w.dom, character)) {
            move_tool(lua, tool, backpack)?;
        }
    }
    Outcome::ret(lua, ())
}

/// `Humanoid:EquipTool(tool)` (creator-docs Humanoid).
pub(super) fn equip_tool(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let tool = a.instance(0)?;
    if !with_world(lua, |w| w.dom.is_a(tool, "Tool")) {
        return Outcome::ret(lua, ());
    }
    let (character, backpack) = with_world(lua, |w| owner(&w.dom, this));
    if let Some(character) = character {
        equip(lua, character, backpack, tool)?;
    }
    Outcome::ret(lua, ())
}

/// Backspace on a client: the local character drops the Tools it holds
/// whose `CanBeDropped` is on into Workspace (measured; the engine does
/// it, not the backpack CoreScript).
pub(crate) fn drop_held(lua: &Lua) {
    let Some(character) = super::local_character(lua) else {
        return;
    };
    let (tools, workspace) = with_world(lua, |w| {
        let dom = &w.dom;
        let tools: Vec<InstanceId> = held_tools(dom, character)
            .into_iter()
            .filter(|t| dom.bool_property(*t, "CanBeDropped") != Some(false))
            .collect();
        (tools, dom.find_service("Workspace"))
    });
    for tool in tools {
        if let Err(e) = move_tool(lua, tool, workspace) {
            tracing::debug!("dropping a tool failed: {e}");
        }
    }
}
