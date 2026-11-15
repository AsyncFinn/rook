//! The Humanoid controller: it holds characters on their feet and walks,
//! jumps, climbs and swims them through Roblox's Humanoid states. Each
//! frame rook-lua hands over a [`Drive`] per Humanoid whose root this
//! world simulates; the controller sets the root body's velocity every
//! 240 Hz step ([`motion`]), senses the floor, trusses and water after the
//! step ([`floor`], [`climb`], [`water`]), moves through the states
//! ([`states`]) and reports what changed ([`Report`]). Measurements:
//! `docs/behavior/characters/movement.md`.

mod climb;
mod controllers;
mod floor;
mod motion;
mod states;
mod water;

use super::*;

use climb::Climb;
use floor::Floor;

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
    /// physics. Seated is one of those (measured: gravity takes it, with or
    /// without a seat; it leans as its seat does, 60 degrees on a 60-degree
    /// seat, and a loose seat falls and lands with its sitter).
    fn upright(self) -> bool {
        !matches!(
            self,
            State::FallingDown
                | State::Ragdoll
                | State::PlatformStanding
                | State::Seated
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

#[derive(Debug)]
pub(crate) struct Controller {
    drive: Drive,
    state: State,
    /// Seconds in the current state.
    time: f32,
    /// Seconds a grounded state has gone without a floor.
    unsupported: f32,
    floor: Option<Floor>,
    climb: Option<Climb>,
    /// The water's surface height, while the root is in water.
    water: Option<f32>,
    /// Seconds a jump's lift still holds gravity off.
    lift: f32,
    /// It left Seated and its root hasn't reached standing height yet.
    rising: bool,
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
            climb: None,
            water: None,
            lift: 0.0,
            rising: false,
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

fn horizontal(v: glam::Vec3) -> glam::Vec3 {
    glam::Vec3::new(v.x, 0.0, v.z)
}
