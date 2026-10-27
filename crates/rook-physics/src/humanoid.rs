//! The Humanoid controller: it holds characters on their feet and walks,
//! jumps, climbs and swims them through Roblox's Humanoid states. Each
//! frame rook-lua hands over a [`Drive`] per Humanoid whose root this
//! world simulates; the controller sets the root body's velocity every
//! 240 Hz step ([`motion`]), senses the floor, trusses and water after the
//! step ([`floor`], [`climb`], [`water`]), moves through the states
//! ([`states`]) and reports what changed ([`Report`]). Measurements:
//! `docs/behavior/characters/movement.md`.

mod climb;
mod floor;
mod motion;
mod states;
mod water;

use super::*;

use climb::Climb;
use floor::Floor;

/// `Enum.HumanoidStateType`, by item name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    FallingDown,
    Ragdoll,
    GettingUp,
    Jumping,
    Swimming,
    Freefall,
    Flying,
    Landed,
    Running,
    RunningNoPhysics,
    StrafingNoPhysics,
    Climbing,
    Seated,
    PlatformStanding,
    Dead,
    Physics,
    None,
}

impl State {
    pub const ALL: [State; 17] = [
        State::FallingDown,
        State::Ragdoll,
        State::GettingUp,
        State::Jumping,
        State::Swimming,
        State::Freefall,
        State::Flying,
        State::Landed,
        State::Running,
        State::RunningNoPhysics,
        State::StrafingNoPhysics,
        State::Climbing,
        State::Seated,
        State::PlatformStanding,
        State::Dead,
        State::Physics,
        State::None,
    ];

    /// The `Enum.HumanoidStateType` item's name.
    pub fn name(self) -> &'static str {
        match self {
            State::FallingDown => "FallingDown",
            State::Ragdoll => "Ragdoll",
            State::GettingUp => "GettingUp",
            State::Jumping => "Jumping",
            State::Swimming => "Swimming",
            State::Freefall => "Freefall",
            State::Flying => "Flying",
            State::Landed => "Landed",
            State::Running => "Running",
            State::RunningNoPhysics => "RunningNoPhysics",
            State::StrafingNoPhysics => "StrafingNoPhysics",
            State::Climbing => "Climbing",
            State::Seated => "Seated",
            State::PlatformStanding => "PlatformStanding",
            State::Dead => "Dead",
            State::Physics => "Physics",
            State::None => "None",
        }
    }

    pub fn from_name(name: &str) -> Option<State> {
        State::ALL.into_iter().find(|s| s.name() == name)
    }

    /// Its bit in [`Drive::disabled`].
    pub fn bit(self) -> u32 {
        1 << State::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    /// States the controller keeps upright; the rest leave the body to
    /// physics.
    fn upright(self) -> bool {
        !matches!(
            self,
            State::FallingDown
                | State::Ragdoll
                | State::PlatformStanding
                | State::Physics
                | State::Dead
                | State::None
        )
    }

    /// States standing on a floor, held at standing height over it.
    fn grounded(self) -> bool {
        matches!(
            self,
            State::Running
                | State::RunningNoPhysics
                | State::StrafingNoPhysics
                | State::Landed
                | State::GettingUp
                | State::Seated
        )
    }
}

/// One Humanoid's settings and requests for a frame, read from its
/// properties.
#[derive(Debug, Clone, PartialEq)]
pub struct Drive {
    pub humanoid: InstanceId,
    pub root: InstanceId,
    /// The root part's size: its footprint is where the floor is felt.
    pub root_size: glam::Vec3,
    /// How high the root's center stands above the floor.
    pub standing_height: f32,
    /// The horizontal velocity walking asks for: MoveDirection, or the
    /// way to WalkToPoint, times WalkSpeed.
    pub walk: glam::Vec3,
    pub walk_speed: f32,
    pub jump: bool,
    /// The launch speed a jump gives (JumpPower, or JumpHeight's).
    pub jump_speed: f32,
    /// MaxSlopeAngle, in radians.
    pub max_slope: f32,
    pub auto_rotate: bool,
    /// With AutoRotate, face this horizontal way rather than the walk's:
    /// the player's camera, while `UserGameSettings.RotationType` is
    /// CameraRelative (first person, shift lock).
    pub face: Option<glam::Vec3>,
    pub platform_stand: bool,
    pub sit: bool,
    /// Health is 0.
    pub dead: bool,
    /// [`State::bit`]s of the states SetStateEnabled turned off.
    pub disabled: u32,
}

/// What a Humanoid did in a step, for its properties and events.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub humanoid: InstanceId,
    /// State changes in order, old then new.
    pub changes: Vec<(State, State)>,
    pub state: State,
    /// The `Enum.Material` value of the floor under it; `None` is Air.
    pub floor_material: Option<u32>,
    /// What its state's speed event reports: Running's speed over the
    /// floor, Climbing's up the truss, Swimming's through the water.
    pub speed: f32,
    /// The controller used or refused the Jump request.
    pub clear_jump: bool,
    /// It got up from sitting.
    pub clear_sit: bool,
}

/// A Humanoid the controller drives.
#[derive(Debug)]
pub(crate) struct Controller {
    drive: Drive,
    state: State,
    /// Seconds in the current state.
    time: f32,
    /// Seconds a grounded state has gone without a floor.
    unsupported: f32,
    floor: Option<Floor>,
    /// Seconds since the floor was sensed, to follow a moving one.
    since_sensed: f32,
    climb: Option<Climb>,
    /// The water's surface height, while the root is in water.
    water: Option<f32>,
    /// Seconds a jump's lift still holds gravity off.
    lift: f32,
    /// A script's ChangeState, applied at the next step.
    requested: Option<State>,
    changes: Vec<(State, State)>,
    clear_jump: bool,
    clear_sit: bool,
}

impl Controller {
    fn new(drive: Drive) -> Self {
        Self {
            drive,
            state: State::Running,
            time: 0.0,
            unsupported: 0.0,
            floor: None,
            since_sensed: 0.0,
            climb: None,
            water: None,
            lift: 0.0,
            requested: None,
            changes: Vec::new(),
            clear_jump: false,
            clear_sit: false,
        }
    }

    fn enabled(&self, state: State) -> bool {
        self.drive.disabled & state.bit() == 0
    }
}

/// The root body of a Humanoid this world simulates, and the root part's
/// pose in it.
struct Rooted {
    body: RigidBodyHandle,
    offset: CFrame,
}

impl Physics {
    /// Humanoids in Workspace, in id order.
    pub fn humanoids(&mut self, dom: &Dom) -> Vec<InstanceId> {
        let ws = dom.find_service("Workspace");
        self.humanoid_ids
            .retain(|&h| !dom.is_destroyed(h) && ws.is_some_and(|ws| dom.is_ancestor_of(ws, h)));
        let mut ids: Vec<InstanceId> = self.humanoid_ids.iter().copied().collect();
        ids.sort_unstable();
        ids
    }

    pub(crate) fn notice_humanoid(&mut self, dom: &Dom, id: InstanceId) {
        match dom.class_name(id) {
            "Humanoid" => {
                self.humanoid_ids.insert(id);
            }
            "TrussPart" => {
                self.trusses.insert(id);
            }
            _ => {}
        }
    }

    /// The controller's state for `humanoid`, if this world drives it.
    pub fn humanoid_state(&self, humanoid: InstanceId) -> Option<State> {
        self.controllers.get(&humanoid).map(|c| c.state)
    }

    /// `Humanoid:ChangeState`: the controller moves to `state` at its next
    /// step, if the state is enabled.
    pub fn change_humanoid_state(&mut self, humanoid: InstanceId, state: State) {
        if let Some(c) = self.controllers.get_mut(&humanoid) {
            c.requested = Some(state);
        }
    }

    /// Take this frame's drives: Humanoids missing from `drives` are no
    /// longer controlled, and their bodies may turn freely again.
    pub fn drive_humanoids(&mut self, drives: Vec<Drive>) {
        let mut old = std::mem::take(&mut self.controllers);
        for drive in drives {
            let id = drive.humanoid;
            let mut c = match old.remove(&id) {
                Some(mut c) => {
                    c.drive = drive;
                    c
                }
                None => Controller::new(drive),
            };
            if let Some(r) = self.rooted(c.drive.root) {
                self.hold(r.body, Some(c.state), false);
                if c.floor.is_none() {
                    self.sense(&mut c, &r);
                }
            }
            self.controllers.insert(id, c);
        }
        for c in old.values() {
            if let Some(r) = self.rooted(c.drive.root) {
                self.hold(r.body, None, false);
            }
        }
    }

    /// The root body of `root`, when this world simulates it.
    fn rooted(&self, root: InstanceId) -> Option<Rooted> {
        let body = self.body_of(root)?;
        let b = self.bodies.get(&body)?;
        if b.remote || b.asm.anchored {
            return None;
        }
        let offset = b.asm.members.iter().find(|m| m.part == root)?.offset;
        Some(Rooted { body, offset })
    }

    /// Hold the body as `state` does: upright states lock its tilt (it may
    /// still turn about Y) and take over its gravity; the rest leave it to
    /// physics. `snap` stands a tilted body up at once (Approximation:
    /// Roblox rises over GettingUp).
    fn hold(&mut self, body: RigidBodyHandle, state: Option<State>, snap: bool) {
        let upright = state.is_some_and(State::upright);
        let rb = &mut self.world.bodies[body];
        let tilt = LockedAxes::ROTATION_LOCKED_X | LockedAxes::ROTATION_LOCKED_Z;
        if upright != rb.locked_axes().contains(tilt) {
            rb.set_enabled_rotations(!upright, true, !upright, true);
        }
        let gravity = if upright { 0.0 } else { 1.0 };
        if rb.gravity_scale() != gravity {
            rb.set_gravity_scale(gravity, true);
        }
        if upright && snap {
            let mut pose = *rb.position();
            let look = pose.rotation * Vector::NEG_Z;
            let yaw = (-look.x).atan2(-look.z);
            pose.rotation = Rotation::from_rotation_y(yaw);
            rb.set_position(pose, true);
            rb.set_angvel(Vector::ZERO, true);
        }
    }

    /// The root part's pose.
    fn root_pose(&self, r: &Rooted) -> CFrame {
        cframe_of(self.world.bodies[r.body].position()) * r.offset
    }

    /// Steer every controlled body for one step of `h` seconds.
    pub(crate) fn steer_humanoids(&mut self, h: f32) {
        if self.controllers.is_empty() {
            return;
        }
        let mut controllers = std::mem::take(&mut self.controllers);
        for c in controllers.values_mut() {
            if let Some(r) = self.rooted(c.drive.root) {
                self.steer(c, &r, h);
            }
        }
        self.controllers = controllers;
    }

    /// After a frame's steps: sense the surroundings and move through the
    /// states.
    pub(crate) fn update_humanoids(&mut self, dt: f32) {
        if self.controllers.is_empty() {
            return;
        }
        let mut controllers = std::mem::take(&mut self.controllers);
        for c in controllers.values_mut() {
            if let Some(r) = self.rooted(c.drive.root) {
                self.sense(c, &r);
                let before = c.state;
                self.transition(c, &r, dt);
                if c.state.upright() != before.upright() {
                    self.hold(r.body, Some(c.state), true);
                }
            }
        }
        self.controllers = controllers;
    }

    /// Sense the floor, a truss or ladder ahead and water.
    fn sense(&self, c: &mut Controller, r: &Rooted) {
        let root = self.root_pose(r);
        c.floor = self.sense_floor(&c.drive, r.body, &root);
        c.since_sensed = 0.0;
        c.climb = self.sense_climb(c, r.body, &root);
        c.water = self.water_surface(root.position);
    }

    /// What each Humanoid did since the last call.
    pub fn humanoid_reports(&mut self, dom: &Dom) -> Vec<Report> {
        let mut controllers = std::mem::take(&mut self.controllers);
        let reports = controllers
            .values_mut()
            .map(|c| {
                let floor_material = c
                    .floor
                    .as_ref()
                    .map(|f| self.material_of(dom, f.collider, f.part));
                Report {
                    humanoid: c.drive.humanoid,
                    changes: std::mem::take(&mut c.changes),
                    state: c.state,
                    floor_material,
                    speed: self.state_speed(c),
                    clear_jump: std::mem::take(&mut c.clear_jump),
                    clear_sit: std::mem::take(&mut c.clear_sit),
                }
            })
            .collect();
        self.controllers = controllers;
        reports
    }

    /// The speed the state's event reports.
    fn state_speed(&self, c: &Controller) -> f32 {
        let Some(r) = self.rooted(c.drive.root) else {
            return 0.0;
        };
        let v = to_glam(self.world.bodies[r.body].linvel());
        let floor = c.floor.as_ref().map_or(glam::Vec3::ZERO, |f| f.velocity);
        match c.state {
            State::Climbing => v.y,
            State::Swimming => v.length(),
            _ => horizontal(v - floor).length(),
        }
    }
}

/// `v` without its Y.
fn horizontal(v: glam::Vec3) -> glam::Vec3 {
    glam::Vec3::new(v.x, 0.0, v.z)
}
