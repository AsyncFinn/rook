//! Network ownership as far as characters: each player's client
//! simulates its own character's assembly (creator-docs "Network
//! ownership"; docs/behavior/characters/ownership.md). `SetNetworkOwner`
//! and automatic ownership of other parts are M10's.

use mlua::Lua;
use rook_dom::InstanceId;
use rook_physics::Authority;
use rook_physics::assembly::Assembly;

use crate::api::methods::ret_instance;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::world::World;

/// The characters of players whose clients simulate them.
fn client_characters(w: &World) -> Vec<(InstanceId, InstanceId)> {
    w.client_players
        .iter()
        .filter_map(|&p| Some((p, w.dom.ref_property(p, "Character")?)))
        .collect()
}

/// What this DataModel's physics simulates: a server everything but its
/// clients' characters, a client only its own character.
pub fn authority(w: &World) -> Authority {
    if w.is_client {
        let own = w
            .dom
            .local_player()
            .and_then(|p| w.dom.ref_property(p, "Character"));
        Authority::Only(own.into_iter().collect())
    } else {
        let mut characters: Vec<InstanceId> =
            client_characters(w).into_iter().map(|(_, c)| c).collect();
        characters.sort();
        Authority::Except(characters)
    }
}

/// The player whose client simulates `part`'s assembly, if any.
fn owner(w: &World, part: InstanceId) -> Option<InstanceId> {
    let asm = Assembly::of(&w.dom, part)?;
    if asm.anchored {
        return None;
    }
    let characters = client_characters(w);
    asm.members.iter().find_map(|m| {
        characters
            .iter()
            .find(|(_, c)| w.dom.is_ancestor_of(*c, m.part))
            .map(|(p, _)| *p)
    })
}

/// Errors Roblox raises for the ownership API (Inference from its
/// messages on SetNetworkOwner; reads behave the same).
fn check(w: &World, part: InstanceId) -> Result<(), ScriptError> {
    if w.is_client {
        return Err(ScriptError(
            "Network Ownership API can only be called from the Server.".into(),
        ));
    }
    let in_workspace = w
        .dom
        .find_service("Workspace")
        .is_some_and(|ws| w.dom.is_ancestor_of(ws, part));
    if !in_workspace {
        return Err(ScriptError(
            "Can only call Network Ownership API on a part that is descendent of Workspace".into(),
        ));
    }
    Ok(())
}

/// `BasePart:GetNetworkOwner()`: the player whose client simulates the
/// part, or nil for the server (measured: a character's parts answer its
/// player).
pub(super) fn get_network_owner(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let owner = with_world(lua, |w| check(w, this).map(|()| owner(w, this)))?;
    ret_instance(lua, owner)
}

/// `BasePart:GetNetworkOwnershipAuto()`: whether the engine decides the
/// owner, true until `SetNetworkOwner` exists (measured: true for a
/// character).
pub(super) fn get_network_ownership_auto(
    lua: &Lua,
    this: InstanceId,
    _: &Args,
) -> ApiResult<Outcome> {
    with_world(lua, |w| check(w, this))?;
    Outcome::ret(lua, true)
}
