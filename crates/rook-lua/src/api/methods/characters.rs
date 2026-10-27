//! Players and their characters: spawning (`spawning`), bodies (`rigs`),
//! looks (`appearance`) and who simulates them (`ownership`).

mod appearance;
pub(crate) mod movement;
pub(crate) mod ownership;
mod rigs;
pub(crate) mod spawning;

use super::*;

pub(crate) use appearance::request as request_avatar;
pub use appearance::{Avatar, Scales};
pub use rigs::{BuildRig, RigSource, RigType};

/// The names a character's root part goes by, in the order they're looked
/// for.
const ROOT_NAMES: [&str; 3] = ["HumanoidRootPart", "Torso", "UpperTorso"];

/// A character's root part, the one answer to "where is this character"
/// (movement, `Humanoid.RootPart`, ClickDetector reach, the client's pose
/// sync, lassos): its model's HumanoidRootPart, else a legacy body's
/// Torso, else an R15 body's UpperTorso (Observed for `RootPart`; a lasso
/// measured starting at the HumanoidRootPart, not the Torso beside it).
pub fn root_part(dom: &rook_dom::Dom, model: InstanceId) -> Option<InstanceId> {
    ROOT_NAMES
        .into_iter()
        .filter_map(|name| dom.find_first_child(model, name, false))
        .find(|&p| dom.is_a(p, "BasePart"))
}

/// `player`'s character's root part.
pub fn character_root(dom: &rook_dom::Dom, player: InstanceId) -> Option<InstanceId> {
    root_part(dom, dom.ref_property(player, "Character")?)
}

/// The root part of `humanoid`'s character.
pub fn humanoid_root(dom: &rook_dom::Dom, humanoid: InstanceId) -> Option<InstanceId> {
    root_part(dom, dom.parent(humanoid)?)
}

/// The Humanoid whose root part `part` is. Cheap for any other part: it
/// looks no further than the part's name.
pub fn humanoid_of_root(dom: &rook_dom::Dom, part: InstanceId) -> Option<InstanceId> {
    if !ROOT_NAMES.contains(&dom.name(part)) {
        return None;
    }
    let model = dom.parent(part)?;
    let humanoid = dom.find_first_child_of_class(model, "Humanoid")?;
    (root_part(dom, model) == Some(part)).then_some(humanoid)
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    if owner == "Humanoid"
        && let Some(m) = movement::native(name)
    {
        return Some(m);
    }
    Some(match (owner, name) {
        ("Players", "GetHumanoidDescriptionFromUserId") => appearance::description_from_user_id,
        ("Humanoid", "ApplyDescription" | "ApplyDescriptionReset") => appearance::apply_description,
        ("Humanoid", "GetAppliedDescription") => appearance::get_applied_description,
        ("BasePart", "GetNetworkOwner") => ownership::get_network_owner,
        ("BasePart", "GetNetworkOwnershipAuto") => ownership::get_network_ownership_auto,
        ("Team", "GetPlayers") => team_players,
        ("Player", "Move") => movement::move_player,
        _ => return None,
    })
}

pub(super) fn players_of(w: &World, this: InstanceId) -> Vec<InstanceId> {
    w.dom
        .children(this)
        .iter()
        .copied()
        .filter(|c| w.dom.is_a(*c, "Player"))
        .collect()
}

pub(super) fn get_players(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    ret_instances(lua, &with_world(lua, |w| players_of(w, this)))
}

/// `Team:GetPlayers()`: the players on it, which only a team in Teams has
/// (measured: the error, for one out of Teams).
fn team_players(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let players = with_world(lua, |w| {
        let dom = &w.dom;
        let in_teams = dom
            .parent(this)
            .is_some_and(|p| dom.class_name(p) == "Teams");
        let all = dom.find_service("Players").map(|p| players_of(w, p));
        in_teams.then(|| {
            (all.unwrap_or_default().into_iter())
                .filter(|&p| dom.ref_property(p, "Team") == Some(this))
                .collect::<Vec<_>>()
        })
    })
    .ok_or_else(|| {
        ScriptError(
            "Team:GetPlayers() failed because team must be a child of the Teams service".into(),
        )
    })?;
    ret_instances(lua, &players)
}

pub(super) fn player_from_character(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let character = a.opt_instance(0)?;
    let found = character.and_then(|c| {
        with_world(lua, |w| {
            players_of(w, this)
                .into_iter()
                .find(|p| w.dom.ref_property(*p, "Character") == Some(c))
        })
    });
    ret_instance(lua, found)
}

pub(super) fn player_by_user_id(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let user_id = a.number(0)? as i64;
    let found = with_world(lua, |w| {
        players_of(w, this)
            .into_iter()
            .find(|p| w.dom.number_property(*p, "UserId") == Some(user_id as f64))
    });
    ret_instance(lua, found)
}

/// Health goes down unless the character has a ForceField (creator-docs
/// Humanoid:TakeDamage). Death handling arrives with characters.
pub(super) fn take_damage(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let amount = a.number(0)?;
    mutate(lua, |w| {
        let protected = w.dom.parent(this).is_some_and(|p| {
            w.dom
                .children(p)
                .iter()
                .any(|c| w.dom.is_a(*c, "ForceField"))
        });
        if protected {
            return Ok(());
        }
        // `Health` is a view of the stored `Health_XML`.
        let health = w.dom.number_property(this, "Health_XML").unwrap_or(0.0);
        let pid = w
            .dom
            .property_id(this, "Health_XML")
            .ok_or(DomError::Stale)?;
        w.dom
            .set_property(this, pid, Value::Float32((health - amount).max(0.0) as f32))
    })?;
    Outcome::ret(lua, ())
}

/// `Player:LoadCharacter()`: the server spawns the player a new character.
pub(super) fn load_character(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    if with_world(lua, |w| w.is_client) {
        return Err(ScriptError(
            "LoadCharacter can only be called by the backend server".into(),
        ));
    }
    spawn_character(lua, this)?;
    Outcome::ret(lua, ())
}

/// Spawn `player` a new character, in creator-docs order (Player:
/// LoadCharacter, CharacterAdded, StarterGui, StarterPack): the old
/// character is removed; the new one is built (`rigs::body`) with
/// StarterCharacterScripts' contents and placed on a spawn; the Backpack
/// and PlayerGui are refilled; `Player.Character` is set and
/// CharacterAdded fires, and only then is the character parented to
/// Workspace.
pub(crate) fn spawn_character(lua: &Lua, player: InstanceId) -> ApiResult<InstanceId> {
    let old = with_world(lua, |w| w.dom.ref_property(player, "Character"));
    if let Some(old) = old {
        let ud = push_instance(lua, old)?;
        fire(
            lua,
            &SignalKey::event(player, "CharacterRemoving"),
            &[LuaValue::UserData(ud)],
        );
        mutate(lua, |w| w.dom.destroy(old))?;
    }
    let character = mutate(lua, |w| {
        let first = w.spawned.insert(player);
        let character = rigs::body(w, player)?;
        movement::apply_starter_settings(&mut w.dom, character)?;
        let (at, spawn) = spawning::spawn_point(w, player, character);
        let dom = &mut w.dom;
        if let Some(root) = root_part(dom, character) {
            rigs::place_at(dom, character, root, at)?;
        }
        // Measured: the character's Humanoid shows the player's DisplayName.
        if let (Some(humanoid), Some(name)) = (
            dom.find_first_child_of_class(character, "Humanoid"),
            dom.string_property(player, "DisplayName"),
        ) {
            dom.set_property_by_name(humanoid, "DisplayName", Value::String(name))?;
        }
        if let Some(scripts) = dom
            .find_service("StarterPlayer")
            .and_then(|sp| dom.find_first_child(sp, "StarterCharacterScripts", false))
        {
            copy_children(dom, scripts, character)?;
        }
        spawning::protect(w, character, spawn)?;
        let dom = &mut w.dom;
        refill_backpack(dom, player)?;
        reset_player_gui(dom, player, first)?;
        dom.set_property_by_name(player, "Character", Value::Ref(Some(character.to_ref())))?;
        Ok(character)
    })?;
    let ud = push_instance(lua, character)?;
    fire(
        lua,
        &SignalKey::event(player, "CharacterAdded"),
        &[LuaValue::UserData(ud)],
    );
    mutate(lua, |w| {
        let workspace = w.dom.find_service("Workspace").ok_or(DomError::Stale)?;
        w.dom.set_parent(character, Some(workspace))
    })?;
    Ok(character)
}

/// Copies of `from`'s archivable children under `to`.
fn copy_children(
    dom: &mut rook_dom::Dom,
    from: InstanceId,
    to: InstanceId,
) -> Result<(), DomError> {
    for child in dom.children(from).to_vec() {
        if let Some(copy) = dom.clone_instance(child) {
            dom.set_parent(copy, Some(to))?;
        }
    }
    Ok(())
}

/// The Backpack holds a fresh copy of StarterPack.
fn refill_backpack(dom: &mut rook_dom::Dom, player: InstanceId) -> Result<(), DomError> {
    let Some(backpack) = dom.find_first_child(player, "Backpack", false) else {
        return Ok(());
    };
    for old in dom.children(backpack).to_vec() {
        dom.destroy(old)?;
    }
    match dom.find_service("StarterPack") {
        Some(pack) => copy_children(dom, pack, backpack),
        None => Ok(()),
    }
}

/// GUIs with `ResetOnSpawn` are replaced by fresh copies from StarterGui;
/// ones without it stay, and aren't copied again (Inference: matched by
/// name). The `first` spawn keeps every GUI the server gave before it
/// (Confirmed in Studio: a ScreenGui parented in PlayerAdded outlives the
/// first character, but not a LoadCharacter after it, even with
/// `Character` nil).
fn reset_player_gui(
    dom: &mut rook_dom::Dom,
    player: InstanceId,
    first: bool,
) -> Result<(), DomError> {
    let Some(gui) = dom.find_first_child(player, "PlayerGui", false) else {
        return Ok(());
    };
    let keeps = |dom: &rook_dom::Dom, id| dom.bool_property(id, "ResetOnSpawn") == Some(false);
    let mut kept = Vec::new();
    for old in dom.children(gui).to_vec() {
        if keeps(dom, old) {
            kept.push(dom.name(old).to_owned());
        } else if !first {
            dom.destroy(old)?;
        }
    }
    let Some(starter) = dom.find_service("StarterGui") else {
        return Ok(());
    };
    for child in dom.children(starter).to_vec() {
        if keeps(dom, child) && kept.iter().any(|k| k == dom.name(child)) {
            continue;
        }
        if let Some(copy) = dom.clone_instance(child) {
            dom.set_parent(copy, Some(gui))?;
        }
    }
    Ok(())
}
