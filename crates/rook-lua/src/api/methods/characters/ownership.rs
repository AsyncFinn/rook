//! Network ownership: which machine simulates each unanchored assembly
//! (creator-docs "Network ownership"; docs/behavior/physics/ownership.md).
//! A player's client simulates its own character and what a server script
//! made it the owner of (`SetNetworkOwner`, kept by physics); the server
//! simulates the rest.

use mlua::Lua;
use rook_dom::InstanceId;
use rook_physics::Authority;
use rook_physics::assembly::Assembly;

use crate::api::methods::ret_instance;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::runtime::world::World;

/// What a client's server made it the owner of, by the client's own ids,
/// which its host keeps up to date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedHere {
    /// Parts of the assemblies a script set this client as the owner of.
    pub parts: Vec<InstanceId>,
    /// Whether this client simulates its character: false once a script
    /// gave it to the server or another player.
    pub character: bool,
}

impl Default for OwnedHere {
    fn default() -> Self {
        Self {
            parts: Vec::new(),
            character: true,
        }
    }
}

/// The characters of players whose clients simulate them.
fn client_characters(w: &World) -> Vec<(InstanceId, InstanceId)> {
    w.client_players
        .iter()
        .filter_map(|&p| Some((p, w.dom.ref_property(p, "Character")?)))
        .collect()
}

/// What this DataModel's physics simulates: a server everything but what
/// its clients own, a client only what it owns.
pub fn authority(w: &mut World) -> Authority {
    if w.is_client {
        let own = w
            .dom
            .local_player()
            .and_then(|p| w.dom.ref_property(p, "Character"))
            .filter(|_| w.owned_here.character);
        let mut owned: Vec<InstanceId> = own.into_iter().collect();
        owned.extend(&w.owned_here.parts);
        owned.sort_unstable();
        owned.dedup();
        return Authority::Only(owned);
    }
    let settings = w.physics.network_settings(&w.dom);
    let set: rustc_hash::FxHashSet<InstanceId> = settings.iter().map(|&(p, _)| p).collect();
    let mut owned: Vec<InstanceId> = client_characters(w)
        .into_iter()
        .filter(|&(p, _)| {
            crate::api::methods::characters::character_root(&w.dom, p)
                .is_none_or(|root| !set.contains(&root))
        })
        .map(|(_, c)| c)
        .collect();
    owned.extend(
        (settings.iter())
            .filter(|(_, owner)| owner.is_some_and(|p| w.client_players.contains(&p)))
            .map(|&(part, _)| part),
    );
    owned.sort_unstable();
    owned.dedup();
    Authority::Except(owned)
}

/// What a client was made the owner of, by the server's ids, for its
/// host to send: the parts a script set `player` as the owner of, sorted,
/// and whether its character is still its own.
pub fn owned_by(w: &mut World, player: InstanceId) -> OwnedHere {
    let settings = w.physics.network_settings(&w.dom);
    let root = crate::api::methods::characters::character_root(&w.dom, player);
    let character = root.is_none_or(|root| {
        settings
            .iter()
            .find(|&&(p, _)| p == root)
            .is_none_or(|&(_, owner)| owner == Some(player))
    });
    let mut parts: Vec<InstanceId> = (settings.iter())
        .filter(|&&(_, owner)| owner == Some(player))
        .map(|&(p, _)| p)
        .collect();
    parts.sort_unstable();
    OwnedHere { parts, character }
}

/// The player whose client simulates `part`'s assembly, if any: none for
/// an anchored one, which the server keeps. A script's setting wins;
/// otherwise a character's assembly is its player's.
pub fn network_owner(w: &mut World, part: InstanceId) -> Option<InstanceId> {
    let asm = Assembly::of(&w.dom, part)?;
    if asm.anchored {
        return None;
    }
    if let Some(owner) = w.physics.network_setting(&w.dom, part) {
        return owner;
    }
    let characters = client_characters(w);
    asm.members.iter().find_map(|m| {
        characters
            .iter()
            .find(|(_, c)| w.dom.is_ancestor_of(*c, m.part))
            .map(|(p, _)| *p)
    })
}

/// The errors Roblox raises for the ownership API, in its order
/// (measured): only on the server, only in Workspace, never for an
/// anchored assembly.
fn check(w: &World, part: InstanceId) -> Result<(), String> {
    if w.is_client {
        return Err("Network Ownership API can only be called from the Server.".into());
    }
    let in_workspace = w
        .dom
        .find_service("Workspace")
        .is_some_and(|ws| w.dom.is_ancestor_of(ws, part));
    if !in_workspace {
        return Err(
            "Can only call Network Ownership API on a part that is descendent of Workspace".into(),
        );
    }
    if Assembly::of(&w.dom, part).is_some_and(|a| a.anchored) {
        return Err("Network Ownership API cannot be called on Anchored parts or parts welded to Anchored parts.".into());
    }
    Ok(())
}

fn checked(lua: &Lua, part: InstanceId) -> ApiResult<()> {
    with_world(lua, |w| check(w, part)).map_err(ScriptError)
}

/// `BasePart:GetNetworkOwner()`: the player whose client simulates the
/// part, or nil for the server (measured: a character's parts answer its
/// player).
pub(super) fn get_network_owner(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    checked(lua, this)?;
    let owner = with_world(lua, |w| network_owner(w, this));
    ret_instance(lua, owner)
}

/// `BasePart:GetNetworkOwnershipAuto()`: whether the engine picks the
/// owner, true until a script sets one (measured).
pub(super) fn get_network_ownership_auto(
    lua: &Lua,
    this: InstanceId,
    _: &Args,
) -> ApiResult<Outcome> {
    checked(lua, this)?;
    let auto = with_world(lua, |w| w.physics.network_setting(&w.dom, this).is_none());
    Outcome::ret(lua, auto)
}

/// `BasePart:SetNetworkOwner(player)`: that player's client simulates the
/// part's whole assembly. Nil, a missing argument or any other Instance
/// gives it to the server (measured).
pub(super) fn set_network_owner(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let player = a.opt_instance(0)?;
    checked(lua, this)?;
    with_world(lua, |w| {
        let owner = player.filter(|&p| w.dom.is_a(p, "Player"));
        w.physics.set_network_owner(&w.dom, this, Some(owner));
    });
    Outcome::ret(lua, ())
}

/// `BasePart:SetNetworkOwnershipAuto()`: the engine picks the owner again.
pub(super) fn set_network_ownership_auto(
    lua: &Lua,
    this: InstanceId,
    _: &Args,
) -> ApiResult<Outcome> {
    checked(lua, this)?;
    with_world(lua, |w| w.physics.set_network_owner(&w.dom, this, None));
    Outcome::ret(lua, ())
}

/// `BasePart:CanSetNetworkOwnership()`: true, or false and the message
/// the other methods would raise (measured).
pub(super) fn can_set_network_ownership(
    lua: &Lua,
    this: InstanceId,
    _: &Args,
) -> ApiResult<Outcome> {
    match with_world(lua, |w| check(w, this)) {
        Ok(()) => Outcome::ret(lua, true),
        Err(message) => Outcome::ret(lua, (false, message)),
    }
}
