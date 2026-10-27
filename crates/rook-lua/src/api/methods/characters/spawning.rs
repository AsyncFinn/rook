//! Where characters appear and which team players are on
//! (docs/behavior/characters/spawning.md).

use glam::Vec3;
use rook_dom::{Dom, DomError, InstanceId};
use rook_physics::{Filter, Region};
use rook_types::{BrickColor, CFrame, Value};

use crate::scheduler::Queue;
use crate::world::{Job, World};

/// Feet land this far above the spawn's top (measured in Studio 0.744:
/// the root part appears 0.01 above where it then stands).
const CLEARANCE: f32 = 0.01;

/// With no spawn a player may use, feet appear this high above the origin
/// (measured).
const NO_SPAWN_HEIGHT: f32 = 100.0;

/// The SpawnLocations `player` may spawn on: enabled ones in Workspace
/// that are neutral, or (for a player on a team) of its team's color
/// (creator-docs SpawnLocation; measured: a neutral player never uses a
/// team's spawn).
fn spawns_for(dom: &Dom, player: InstanceId) -> Vec<InstanceId> {
    let Some(ws) = dom.find_service("Workspace") else {
        return Vec::new();
    };
    let team_color = (dom.bool_property(player, "Neutral") == Some(false))
        .then(|| dom.brick_color_property(player, "TeamColor"))
        .flatten();
    dom.descendants(ws)
        .into_iter()
        .filter(|&s| dom.is_a(s, "SpawnLocation"))
        .filter(|&s| dom.bool_property(s, "Enabled") != Some(false))
        .filter(|&s| {
            dom.bool_property(s, "Neutral") != Some(false)
                || team_color.is_some() && dom.brick_color_property(s, "TeamColor") == team_color
        })
        .collect()
}

/// Where `body`'s root part goes for `player`, and the spawn it's on: a
/// random one of the spawns it may use, a random spot on its top (1 stud
/// in from the edges; measured: spawns 6 studs across put players at the
/// center, where to switch is Inference), raised clear of other parts the
/// body would stand in.
pub(super) fn spawn_point(
    w: &mut World,
    player: InstanceId,
    body: InstanceId,
) -> (CFrame, Option<InstanceId>) {
    let drop = feet_below_root(&w.dom, body);
    let spawns = spawns_for(&w.dom, player);
    let picked = (!spawns.is_empty())
        .then(|| spawns[(w.spawn_random.next_f64() * spawns.len() as f64) as usize % spawns.len()]);
    let at = picked.and_then(|s| {
        let cf = w.dom.cframe_property(s, "CFrame")?;
        let size = w.dom.vector3_property(s, "Size")?;
        let mut offset = |extent: f32| {
            let range = extent / 2.0 - 1.0;
            let r = w.spawn_random.next_f64() as f32 * 2.0 - 1.0;
            if extent > 6.0 { r * range } else { 0.0 }
        };
        let (x, z) = (offset(size.x), offset(size.z));
        let top = cf * CFrame::from_position(Vec3::new(x, size.y / 2.0, z));
        Some((top.position, cf.rotation * Vec3::NEG_Z))
    });
    let (feet, look) = at.unwrap_or((Vec3::new(0.0, NO_SPAWN_HEIGHT, 0.0), Vec3::NEG_Z));
    let mut root = feet + Vec3::new(0.0, drop + CLEARANCE, 0.0);
    root.y = clear_of_parts(w, body, root, picked);
    // Measured: the character faces the way its spawn does. Inference: a
    // tilted spawn's way, kept level.
    let level = Vec3::new(look.x, 0.0, look.z)
        .try_normalize()
        .unwrap_or(Vec3::NEG_Z);
    (CFrame::look_at(root, root + level, Vec3::Y), picked)
}

/// How far a part reaches from its center along each world axis.
fn reach(dom: &Dom, part: InstanceId) -> Option<(Vec3, Vec3)> {
    let cf = dom.cframe_property(part, "CFrame")?;
    let half = dom.vector3_property(part, "Size")? / 2.0;
    let r = cf.rotation;
    Some((
        cf.position,
        (r * Vec3::X * half.x).abs() + (r * Vec3::Y * half.y).abs() + (r * Vec3::Z * half.z).abs(),
    ))
}

/// The body's parts, each with its center and reach.
fn body_parts(dom: &Dom, body: InstanceId) -> Vec<(Vec3, Vec3)> {
    dom.descendants(body)
        .into_iter()
        .filter(|&p| dom.is_a(p, "BasePart"))
        .filter_map(|p| reach(dom, p))
        .collect()
}

/// How far the lowest point of `body` is below its root part.
pub(super) fn feet_below_root(dom: &Dom, body: InstanceId) -> f32 {
    let root = super::root_part(dom, body);
    let Some(root_y) = root
        .and_then(|r| dom.cframe_property(r, "CFrame"))
        .map(|c| c.position.y)
    else {
        return 3.0;
    };
    let lowest = body_parts(dom, body)
        .into_iter()
        .map(|(at, reach)| at.y - reach.y)
        .fold(root_y, f32::min);
    root_y - lowest
}

/// The root part's height once the body's box clears the parts that
/// collide in it, besides the spawn (Inference from Studio: a player
/// spawned into a block on a spawn ends up standing on the block).
fn clear_of_parts(w: &mut World, body: InstanceId, root: Vec3, spawn: Option<InstanceId>) -> f32 {
    let size = extents(&w.dom, body).unwrap_or(Vec3::new(4.0, 5.0, 1.0));
    let center_above_root = size.y / 2.0 - feet_below_root(&w.dom, body);
    let mut y = root.y;
    for _ in 0..8 {
        let cframe = CFrame::from_position(Vec3::new(root.x, y + center_above_root, root.z));
        let World { dom, physics, .. } = w;
        let hits =
            physics.parts_in_bounds(dom, Region::Box { cframe, size }, &Filter::default(), 64);
        let top = hits
            .into_iter()
            .filter(|&p| Some(p) != spawn && dom.bool_property(p, "CanCollide") != Some(false))
            .filter_map(|p| reach(dom, p).map(|(at, reach)| at.y + reach.y))
            .fold(f32::NEG_INFINITY, f32::max);
        if !top.is_finite() || top <= y - feet_below_root(dom, body) {
            break;
        }
        y = top + feet_below_root(dom, body) + CLEARANCE;
    }
    y
}

/// The size of the box around `body`'s parts.
fn extents(dom: &Dom, body: InstanceId) -> Option<Vec3> {
    let parts = body_parts(dom, body);
    let lo = parts.iter().map(|(at, r)| at - r).reduce(Vec3::min)?;
    let hi = parts.iter().map(|(at, r)| at + r).reduce(Vec3::max)?;
    Some(hi - lo)
}

/// A spawn with a `Duration` protects the new character with a ForceField
/// for that many seconds (creator-docs SpawnLocation.Duration; observed:
/// a 0 duration gives none).
pub(super) fn protect(
    w: &mut World,
    character: InstanceId,
    spawn: Option<InstanceId>,
) -> Result<(), DomError> {
    let Some(duration) = spawn.and_then(|s| w.dom.number_property(s, "Duration")) else {
        return Ok(());
    };
    if duration <= 0.0 {
        return Ok(());
    }
    let field = w.dom.create_by_name("ForceField")?;
    w.dom.set_parent(field, Some(character))?;
    w.scheduler
        .schedule(Queue::Task, duration, Job::Destroy(field));
    Ok(())
}

/// Put a joining player on the auto-assignable team with the fewest
/// players, the first such team on a tie (creator-docs Team.AutoAssignable;
/// the tie rule is Inference). Players with no such team stay neutral.
pub(crate) fn assign_team(dom: &mut Dom, player: InstanceId) -> Result<(), DomError> {
    let Some(teams) = dom.find_service("Teams") else {
        return Ok(());
    };
    let players = dom
        .find_service("Players")
        .map(|p| dom.children(p).to_vec())
        .unwrap_or_default();
    let count = |team: InstanceId| {
        players
            .iter()
            .filter(|&&p| p != player && dom.ref_property(p, "Team") == Some(team))
            .count()
    };
    let team = dom
        .children(teams)
        .iter()
        .copied()
        .filter(|&t| dom.is_a(t, "Team") && dom.bool_property(t, "AutoAssignable") != Some(false))
        .min_by_key(|&t| count(t));
    match team {
        Some(team) => join_team(dom, player, Some(team)),
        None => Ok(()),
    }
}

/// Set a player's `Team`, with `TeamColor` and `Neutral` following
/// (creator-docs Player.Team).
pub(crate) fn join_team(
    dom: &mut Dom,
    player: InstanceId,
    team: Option<InstanceId>,
) -> Result<(), DomError> {
    dom.set_property_by_name(player, "Team", Value::Ref(team.map(InstanceId::to_ref)))?;
    dom.set_property_by_name(player, "Neutral", Value::Bool(team.is_none()))?;
    if let Some(color) = team.and_then(|t| dom.brick_color_property(t, "TeamColor")) {
        dom.set_property_by_name(player, "TeamColor", Value::BrickColor(color))?;
    }
    Ok(())
}

/// Set a player's `TeamColor`: it joins the first team of that color, or
/// (measured) is on none with `Neutral` as it was.
pub(crate) fn take_color(
    dom: &mut Dom,
    player: InstanceId,
    color: BrickColor,
) -> Result<(), DomError> {
    match team_of_color(dom, color) {
        Some(team) => join_team(dom, player, Some(team)),
        None => dom.set_property_by_name(player, "Team", Value::Ref(None)),
    }?;
    dom.set_property_by_name(player, "TeamColor", Value::BrickColor(color))
}

/// Set a player's `Neutral`: true leaves its team; false (measured) joins
/// the first team of its `TeamColor`, if any.
pub(crate) fn set_neutral(
    dom: &mut Dom,
    player: InstanceId,
    neutral: bool,
) -> Result<(), DomError> {
    if neutral {
        return join_team(dom, player, None);
    }
    dom.set_property_by_name(player, "Neutral", Value::Bool(false))?;
    let team = dom
        .brick_color_property(player, "TeamColor")
        .and_then(|c| team_of_color(dom, c));
    dom.set_property_by_name(player, "Team", Value::Ref(team.map(InstanceId::to_ref)))
}

/// Players whose team left Teams or changed color are moved to the first
/// team of their `TeamColor`, or none (measured in Studio 0.744: a
/// recolored team loses its players; one taken out of Teams hands them to
/// another of the same color). Checked once a frame.
pub(crate) fn keep_teams(dom: &mut Dom) -> Result<(), DomError> {
    let (Some(players), teams) = (dom.find_service("Players"), dom.find_service("Teams")) else {
        return Ok(());
    };
    for player in dom.children(players).to_vec() {
        let Some(team) = dom.ref_property(player, "Team") else {
            continue;
        };
        let color = dom.brick_color_property(player, "TeamColor");
        let kept = teams.is_some_and(|t| dom.parent(team) == Some(t))
            && dom.brick_color_property(team, "TeamColor") == color;
        if !kept {
            let next = color.and_then(|c| team_of_color(dom, c));
            dom.set_property_by_name(player, "Team", Value::Ref(next.map(InstanceId::to_ref)))?;
        }
    }
    Ok(())
}

/// The team of a color, the first in Teams.
pub(crate) fn team_of_color(dom: &Dom, color: BrickColor) -> Option<InstanceId> {
    let teams = dom.find_service("Teams")?;
    dom.children(teams)
        .iter()
        .copied()
        .find(|&t| dom.is_a(t, "Team") && dom.brick_color_property(t, "TeamColor") == Some(color))
}

/// A character touched a SpawnLocation with `AllowTeamChangeOnTouch`: its
/// player joins the spawn's team (creator-docs).
pub(crate) fn spawn_touched(
    dom: &mut Dom,
    spawn: InstanceId,
    part: InstanceId,
) -> Result<(), DomError> {
    if dom.bool_property(spawn, "AllowTeamChangeOnTouch") != Some(true)
        || dom.bool_property(spawn, "Neutral") != Some(false)
    {
        return Ok(());
    }
    let Some(players) = dom.find_service("Players") else {
        return Ok(());
    };
    let player = dom.children(players).iter().copied().find(|&p| {
        dom.ref_property(p, "Character")
            .is_some_and(|c| dom.is_ancestor_of(c, part))
    });
    let (Some(player), Some(color)) = (player, dom.brick_color_property(spawn, "TeamColor")) else {
        return Ok(());
    };
    match team_of_color(dom, color) {
        Some(team) if dom.ref_property(player, "Team") != Some(team) => {
            join_team(dom, player, Some(team))
        }
        _ => Ok(()),
    }
}
