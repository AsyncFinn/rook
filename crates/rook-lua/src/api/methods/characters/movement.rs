//! Humanoid movement: `Move`, `MoveTo` and the state methods; the drives
//! the physics controller takes each frame (`rook_physics::humanoid`); and
//! what its reports change and fire. Measurements:
//! docs/behavior/characters/movement.md.

use mlua::{Lua, Value as LuaValue};
use rook_dom::{Dom, DomError, InstanceId};
use rook_physics::humanoid::{Drive, Report, State};
use rook_reflection::database;
use rook_types::{Value, Vector3};

use crate::api::methods::mutate;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};
use crate::signal::SignalKey;
use crate::threads::fire;
use crate::types::enums::{enum_item, item_name};
use crate::world::World;

/// MoveTo gives up after this long (measured: MoveToFinished(false) 8 s
/// after a walk into a wall).
const WALK_TIMEOUT: f64 = 8.0;

/// MoveTo arrives once the root is this close across the ground
/// (measured: it stopped 0.93 studs short, then slid to 0.8).
const ARRIVED: f32 = 1.0;

/// A speed event fires again once the speed has changed by this much
/// (Approximation: Roblox's threshold isn't documented).
const SPEED_STEP: f32 = 0.1;

/// `Enum.Material.Air`, the FloorMaterial with no floor.
const AIR: u32 = 1792;

/// A Humanoid's MoveTo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Walk {
    point: glam::Vec3,
    part: Option<InstanceId>,
    started: f64,
    active: bool,
}

/// Script state the controller's drives carry.
#[derive(Debug, Default)]
pub(crate) struct Movement {
    walks: rustc_hash::FxHashMap<InstanceId, Walk>,
    /// States SetStateEnabled turned off, as `State::bit`s.
    disabled: rustc_hash::FxHashMap<InstanceId, u32>,
    /// The speed each Humanoid's speed event last reported.
    speeds: rustc_hash::FxHashMap<InstanceId, f32>,
}

pub(super) fn native(name: &str) -> Option<super::RawMethod> {
    Some(match name {
        "Move" => move_humanoid,
        "MoveTo" => move_to,
        "GetState" => get_state,
        "ChangeState" => change_state,
        "SetStateEnabled" => set_state_enabled,
        "GetStateEnabled" => get_state_enabled,
        _ => return None,
    })
}

fn set(dom: &mut Dom, id: InstanceId, name: &str, value: Value) -> Result<(), DomError> {
    dom.set_property_by_name(id, name, value)
}

/// `Humanoid:Move(direction, relativeToCamera)`: MoveDirection, flat and
/// at most 1 long, turned by the camera's heading when asked. A direction
/// cancels a MoveTo (measured).
fn move_humanoid(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let direction = a.vector3(0)?.0;
    let relative = a.truthy(1);
    mutate(lua, |w| {
        let mut d = direction;
        if relative && let Some(camera) = current_camera(&w.dom) {
            let flat = |v: glam::Vec3| glam::Vec3::new(v.x, 0.0, v.z).normalize_or_zero();
            let right = flat(camera.rotation * glam::Vec3::X);
            let back = flat(camera.rotation * glam::Vec3::Z);
            d = right * d.x + back * d.z;
        }
        d.y = 0.0;
        let d = d.clamp_length_max(1.0);
        if d != glam::Vec3::ZERO
            && let Some(walk) = w.movement.walks.get_mut(&this)
        {
            walk.active = false;
        }
        set(
            &mut w.dom,
            this,
            "MoveDirection",
            Value::Vector3(Vector3(d)),
        )
    })?;
    Outcome::ret(lua, ())
}

fn current_camera(dom: &Dom) -> Option<rook_types::CFrame> {
    let ws = dom.find_service("Workspace")?;
    let camera = dom.ref_property(ws, "CurrentCamera")?;
    dom.cframe_property(camera, "CFrame")
}

/// `Humanoid:MoveTo(location, part)`: walk there, or to that spot on the
/// part as it moves (WalkToPoint is then in the part's space, as
/// creator-docs describe).
fn move_to(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let location = a.vector3(0)?.0;
    let part = a.opt_instance(1)?;
    mutate(lua, |w| {
        let part = part.filter(|&p| w.dom.is_a(p, "BasePart"));
        let point = match part.and_then(|p| w.dom.cframe_property(p, "CFrame")) {
            Some(cf) => cf.point_to_object_space(Vector3(location)).0,
            None => location,
        };
        let walk = Walk {
            point,
            part,
            started: w.scheduler.now(),
            active: true,
        };
        w.movement.walks.insert(this, walk);
        set(
            &mut w.dom,
            this,
            "WalkToPoint",
            Value::Vector3(Vector3(point)),
        )?;
        let part_ref = part.map(InstanceId::to_ref);
        set(&mut w.dom, this, "WalkToPart", Value::Ref(part_ref))
    })?;
    Outcome::ret(lua, ())
}

fn state_item(lua: &Lua, state: State) -> mlua::Result<LuaValue> {
    let value = database()
        .enum_by_name("HumanoidStateType")
        .and_then(|e| e.item_value(state.name()))
        .unwrap_or(0);
    enum_item(lua, "HumanoidStateType", value)
}

fn state_arg(lua: &Lua, a: &Args, i: usize) -> ApiResult<State> {
    item_name(lua, a.get(i), "HumanoidStateType")
        .and_then(State::from_name)
        .ok_or_else(|| a.wrong(i, "HumanoidStateType"))
}

/// `Humanoid:GetState()`: the controller's state; a Humanoid this
/// DataModel doesn't simulate reads Running, or Dead without health
/// (Inference).
fn get_state(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let state = with_world(lua, |w| {
        w.physics.humanoid_state(this).unwrap_or_else(|| {
            if w.dom
                .number_property(this, "Health_XML")
                .is_some_and(|h| h <= 0.0)
            {
                State::Dead
            } else {
                State::Running
            }
        })
    });
    Outcome::ret(lua, state_item(lua, state)?)
}

/// `Humanoid:ChangeState(state)`: the controller goes there at its next
/// step, if the state is enabled.
fn change_state(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let state = state_arg(lua, a, 0)?;
    with_world(lua, |w| w.physics.change_humanoid_state(this, state));
    Outcome::ret(lua, ())
}

fn set_state_enabled(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let state = state_arg(lua, a, 0)?;
    let enabled = a.truthy(1);
    let changed = with_world(lua, |w| {
        let bits = w.movement.disabled.entry(this).or_default();
        let before = *bits;
        if enabled {
            *bits &= !state.bit();
        } else {
            *bits |= state.bit();
        }
        before != *bits
    });
    if changed {
        let args = [state_item(lua, state)?, LuaValue::Boolean(enabled)];
        fire(lua, &SignalKey::event(this, "StateEnabledChanged"), &args);
    }
    Outcome::ret(lua, ())
}

fn get_state_enabled(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let state = state_arg(lua, a, 0)?;
    let disabled = with_world(lua, |w| {
        w.movement.disabled.get(&this).copied().unwrap_or(0)
    });
    Outcome::ret(lua, disabled & state.bit() == 0)
}

/// A Humanoid's root part: HumanoidRootPart, or a legacy body's Torso
/// (Inference from creator-docs Humanoid.RootPart).
fn root_part(dom: &Dom, model: InstanceId) -> Option<InstanceId> {
    dom.find_first_child(model, "HumanoidRootPart", false)
        .or_else(|| dom.find_first_child(model, "Torso", false))
        .filter(|&p| dom.is_a(p, "BasePart"))
}

/// How high a root part's center stands above the floor: HipHeight from
/// the root's bottom on R15 bodies; R6 bodies add their legs, with
/// HipHeight usually 0 (creator-docs HipHeight; measured 3.0075 for
/// Studio's default R15 body).
fn standing_height(
    dom: &Dom,
    humanoid: InstanceId,
    model: InstanceId,
    root_size: glam::Vec3,
) -> f32 {
    let hip = dom.float_property(humanoid, "HipHeight").unwrap_or(0.0);
    let r15 = dom.enum_item_property(humanoid, "RigType") == Some("R15");
    let legs = if r15 {
        0.0
    } else {
        dom.find_first_child(model, "Left Leg", false)
            .and_then(|l| dom.vector3_property(l, "Size"))
            .map_or(2.0, |s| s.y)
    };
    hip + root_size.y * 0.5 + legs
}

/// The drives for every Humanoid whose root this DataModel simulates.
pub fn drives(w: &mut World) -> Vec<Drive> {
    let ids = w.physics.humanoids(&w.dom);
    let now = w.scheduler.now();
    let gravity = (w.dom.find_service("Workspace"))
        .and_then(|ws| w.dom.float_property(ws, "Gravity"))
        .unwrap_or(196.2);
    let mut drives = Vec::new();
    for humanoid in ids {
        let dom = &w.dom;
        let Some(model) = dom.parent(humanoid).filter(|&m| dom.is_a(m, "Model")) else {
            continue;
        };
        let Some(root) = root_part(dom, model) else {
            continue;
        };
        if !w.physics.simulates(root) {
            continue;
        }
        let (Some(root_cf), Some(root_size)) = (
            dom.cframe_property(root, "CFrame"),
            dom.vector3_property(root, "Size"),
        ) else {
            continue;
        };
        let f = |name, default| dom.float_property(humanoid, name).unwrap_or(default);
        let b = |name, default| dom.bool_property(humanoid, name).unwrap_or(default);
        let walk_speed = f("WalkSpeed", 16.0).max(0.0);
        // A changed WalkToPoint or WalkToPart starts a walk, as MoveTo does
        // (Inference: both are writable, and replicate from a server).
        let point = dom
            .vector3_property(humanoid, "WalkToPoint")
            .unwrap_or_default();
        let part = dom.ref_property(humanoid, "WalkToPart");
        let walk = w.movement.walks.entry(humanoid).or_insert(Walk {
            point,
            part,
            started: now,
            active: false,
        });
        if walk.point != point || walk.part != part {
            *walk = Walk {
                point,
                part,
                started: now,
                active: true,
            };
        }
        let target = walk.active.then(|| walk_target(dom, walk));
        let dir = match target {
            Some(t) => {
                let d = t - root_cf.position;
                glam::Vec3::new(d.x, 0.0, d.z).normalize_or_zero()
            }
            None => dom
                .vector3_property(humanoid, "MoveDirection")
                .unwrap_or_default(),
        };
        let jump_speed = if b("UseJumpPower", true) {
            f("JumpPower", 50.0)
        } else {
            (2.0 * gravity * f("JumpHeight", 7.2)).max(0.0).sqrt()
        };
        drives.push(Drive {
            humanoid,
            root,
            root_size,
            standing_height: standing_height(dom, humanoid, model, root_size),
            walk: dir * walk_speed,
            walk_speed,
            jump: b("Jump", false),
            jump_speed,
            max_slope: f("MaxSlopeAngle", 89.0).to_radians(),
            auto_rotate: b("AutoRotate", true),
            platform_stand: b("PlatformStand", false),
            sit: b("Sit", false),
            dead: dom
                .number_property(humanoid, "Health_XML")
                .is_some_and(|h| h <= 0.0),
            disabled: w.movement.disabled.get(&humanoid).copied().unwrap_or(0),
        });
    }
    drives
}

/// Where a walk goes: WalkToPoint, in WalkToPart's space when it has one.
fn walk_target(dom: &Dom, walk: &Walk) -> glam::Vec3 {
    match walk.part.and_then(|p| dom.cframe_property(p, "CFrame")) {
        Some(cf) => cf.point_to_world_space(Vector3(walk.point)).0,
        None => walk.point,
    }
}

/// Apply the controller's reports: FloorMaterial, Jump and Sit, finished
/// walks, and the state events.
pub(crate) fn stepped(lua: &Lua, reports: Vec<Report>) -> ApiResult<()> {
    for r in reports {
        let id = r.humanoid;
        let finished = mutate(lua, |w| {
            let material = r.floor_material.unwrap_or(AIR);
            if w.dom.enum_property(id, "FloorMaterial") != Some(material) {
                set(&mut w.dom, id, "FloorMaterial", Value::Enum(material))?;
            }
            if r.clear_jump {
                set(&mut w.dom, id, "Jump", Value::Bool(false))?;
            }
            if r.clear_sit {
                set(&mut w.dom, id, "Sit", Value::Bool(false))?;
            }
            Ok(walk_finished(w, id))
        })?;
        if let Some(reached) = finished {
            fire(
                lua,
                &SignalKey::event(id, "MoveToFinished"),
                &[LuaValue::Boolean(reached)],
            );
        }
        fire_state_events(lua, &r)?;
    }
    Ok(())
}

/// Whether the Humanoid's walk ended this frame, arriving or not.
fn walk_finished(w: &mut World, humanoid: InstanceId) -> Option<bool> {
    let now = w.scheduler.now();
    let dom = &w.dom;
    let walk = w.movement.walks.get_mut(&humanoid).filter(|w| w.active)?;
    let model = dom.parent(humanoid)?;
    let root = dom
        .cframe_property(root_part(dom, model)?, "CFrame")?
        .position;
    let d = walk_target(dom, walk) - root;
    let reached = glam::Vec2::new(d.x, d.z).length() < ARRIVED;
    if reached || now - walk.started >= WALK_TIMEOUT {
        walk.active = false;
        return Some(reached);
    }
    None
}

/// StateChanged and each state's own event: the `active` events fire true
/// on entering and false on leaving, and the speed events on entering and
/// as the speed changes (Inference from creator-docs' event list).
fn fire_state_events(lua: &Lua, r: &Report) -> mlua::Result<()> {
    let id = r.humanoid;
    for &(old, new) in &r.changes {
        let args = [state_item(lua, old)?, state_item(lua, new)?];
        fire(lua, &SignalKey::event(id, "StateChanged"), &args);
        if let Some(event) = active_event(old) {
            fire(lua, &SignalKey::event(id, event), &active_args(old, false));
        }
        if let Some(event) = active_event(new) {
            fire(lua, &SignalKey::event(id, event), &active_args(new, true));
        }
        if speed_event(new).is_some() {
            with_world(lua, |w| w.movement.speeds.remove(&id));
        }
    }
    let Some(event) = speed_event(r.state) else {
        return Ok(());
    };
    let changed = with_world(lua, |w| {
        let last = w.movement.speeds.get(&id).copied();
        let changed = last.is_none_or(|s| (s - r.speed).abs() > SPEED_STEP);
        if changed {
            w.movement.speeds.insert(id, r.speed);
        }
        changed
    });
    if changed {
        fire(
            lua,
            &SignalKey::event(id, event),
            &[LuaValue::Number(f64::from(r.speed))],
        );
    }
    Ok(())
}

fn active_event(state: State) -> Option<&'static str> {
    Some(match state {
        State::Jumping => "Jumping",
        State::Freefall => "FreeFalling",
        State::FallingDown => "FallingDown",
        State::GettingUp => "GettingUp",
        State::Ragdoll => "Ragdoll",
        State::PlatformStanding => "PlatformStanding",
        State::Seated => "Seated",
        _ => return None,
    })
}

fn active_args(state: State, active: bool) -> Vec<LuaValue> {
    let mut args = vec![LuaValue::Boolean(active)];
    if state == State::Seated {
        // The seat comes with seats (M9's seats step).
        args.push(LuaValue::Nil);
    }
    args
}

fn speed_event(state: State) -> Option<&'static str> {
    Some(match state {
        State::Running | State::RunningNoPhysics => "Running",
        State::Climbing => "Climbing",
        State::Swimming => "Swimming",
        _ => return None,
    })
}

/// A spawned character takes StarterPlayer's character settings
/// (creator-docs StarterPlayer: CharacterWalkSpeed and the rest apply to
/// each new character's Humanoid).
pub(super) fn apply_starter_settings(dom: &mut Dom, character: InstanceId) -> Result<(), DomError> {
    let (Some(sp), Some(humanoid)) = (
        dom.find_service("StarterPlayer"),
        dom.find_first_child_of_class(character, "Humanoid"),
    ) else {
        return Ok(());
    };
    for (from, to) in [
        ("CharacterWalkSpeed", "WalkSpeed"),
        ("CharacterJumpPower", "JumpPower"),
        ("CharacterJumpHeight", "JumpHeight"),
        ("CharacterUseJumpPower", "UseJumpPower"),
        ("CharacterMaxSlopeAngle", "MaxSlopeAngle"),
        ("AutoJumpEnabled", "AutoJumpEnabled"),
    ] {
        let Some(id) = dom.property_id(sp, from) else {
            continue;
        };
        if let Some(value) = dom.property(sp, id) {
            set(dom, humanoid, to, value)?;
        }
    }
    Ok(())
}
