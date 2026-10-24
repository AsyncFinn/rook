//! Players and Humanoids.

use super::*;

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
        let health = w.dom.number_property(this, "Health").unwrap_or(0.0);
        let pid = w.dom.property_id(this, "Health").ok_or(DomError::Stale)?;
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
/// character is removed; the new one is built with StarterCharacterScripts'
/// contents; the Backpack and PlayerGui are refilled; `Player.Character` is
/// set and CharacterAdded fires, and only then is the character parented
/// to Workspace.
///
/// The body is a Humanoid and an invisible, anchored HumanoidRootPart that
/// nothing collides with or touches, until rigs, walking and physics come.
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
        let dom = &mut w.dom;
        let character = build_body(dom, player)?;
        if let Some(scripts) = dom
            .find_service("StarterPlayer")
            .and_then(|sp| dom.find_first_child(sp, "StarterCharacterScripts", false))
        {
            copy_children(dom, scripts, character)?;
        }
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

/// A Model named after the player holding a Humanoid and its root part,
/// standing on the first enabled SpawnLocation.
fn build_body(dom: &mut rook_dom::Dom, player: InstanceId) -> Result<InstanceId, DomError> {
    let model = dom.create_by_name("Model")?;
    dom.set_name(model, dom.name(player).to_owned())?;
    let humanoid = dom.create_by_name("Humanoid")?;
    dom.set_parent(humanoid, Some(model))?;
    let root = dom.create_by_name("Part")?;
    dom.set_name(root, "HumanoidRootPart".into())?;
    let at = spawn_point(dom);
    for (name, value) in [
        ("Size", Value::Vector3(Vector3::new(2.0, 2.0, 1.0))),
        ("CFrame", Value::CFrame(CFrame::from_position(at))),
        ("Transparency", Value::Float32(1.0)),
        ("Anchored", Value::Bool(true)),
        ("CanCollide", Value::Bool(false)),
        ("CanTouch", Value::Bool(false)),
        ("CanQuery", Value::Bool(false)),
    ] {
        dom.set_property_by_name(root, name, value)?;
    }
    dom.set_parent(root, Some(model))?;
    dom.set_property_by_name(model, "PrimaryPart", Value::Ref(Some(root.to_ref())))?;
    Ok(model)
}

/// Where a root part spawns: 3 studs above the top of the first enabled
/// SpawnLocation in Workspace, or 3 above the origin without one
/// (Inference: Roblox picks randomly among the ones the player's team
/// may use; teams come with M9).
fn spawn_point(dom: &rook_dom::Dom) -> glam::Vec3 {
    let spawn = dom.find_service("Workspace").and_then(|ws| {
        dom.find_descendant(ws, &|d| {
            dom.is_a(d, "SpawnLocation") && dom.bool_property(d, "Enabled") != Some(false)
        })
    });
    let Some(spawn) = spawn else {
        return glam::Vec3::new(0.0, 3.0, 0.0);
    };
    let (Some(cf), Some(size)) = (
        dom.cframe_property(spawn, "CFrame"),
        dom.vector3_property(spawn, "Size"),
    ) else {
        return glam::Vec3::new(0.0, 3.0, 0.0);
    };
    cf.position + glam::Vec3::new(0.0, size.y / 2.0 + 3.0, 0.0)
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
