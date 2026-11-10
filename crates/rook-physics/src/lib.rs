//! Roblox physics semantics over Rapier. Each world simulates the
//! assemblies its machine owns ([`Authority`]); the rest follow where
//! replication puts them. Findings are in `docs/behavior/physics/`.

pub mod assembly;
pub mod contacts;
mod convert;
mod explosion;
pub mod humanoid;
pub mod joints;
pub mod query;
mod remote;
mod shape;
mod step;
mod sync;

use std::collections::{BTreeMap, BTreeSet};

use rustc_hash::{FxHashMap, FxHashSet};

use rapier3d::prelude::*;
use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::PropertyId;
use rook_types::{CFrame, Value, Vector3};

pub use shape::{is_part, part_box, part_shape, workspace_parts};

pub use explosion::Blast;
pub use query::{CastShape, Filter, Hit, Region};

use assembly::bodies::{Body, Home, Static};
use assembly::{Assembly, is_massless};
use contacts::groups::CollisionGroups;
use convert::{between, cframe_of, pose_of};
use joints::constraints::{Drive, Spec, apply_drive, hinge_constraint_drive, revolute_angle};
use joints::drives::{BuiltConstraint, DriveSource, Driven};
use joints::movers::Mover;
use joints::{Joint, JointKind, active_joint, endpoints, is_joint};
use sync::props::Props;

use contacts::{groups, materials};
use joints::{constraints, movers};
pub use shape::saved::saved_blob;
use shape::{saved, terrain};

/// Roblox simulates at 240 Hz (creator-docs "Adaptive timestepping": 240,
/// 120 or 60 Hz per assembly; Rook always uses 240, an Approximation).
pub const STEP_HZ: f64 = 240.0;

/// Size the worker threads collision detection and the solver use. The
/// engine gives physics its share of the machine (half the cores), so a
/// server stepping physics leaves the rest to the client, the renderer and
/// content decoding; without a call, Rayon's default (every core) stands.
/// Only the first call counts. It's Rayon's global pool, not
/// a pool of the world's own: Rapier hands a whole step to a world's own
/// pool, a cross-thread wake-up per 240 Hz step that made small places
/// slower. Rapier lays the parallel work out the same whatever the pool's
/// size, so results don't depend on it.
pub fn size_worker_pool(threads: usize) {
    static SIZED: std::sync::Once = std::sync::Once::new();
    SIZED.call_once(|| {
        // Fails only if something else built the global pool first; then
        // that size stands.
        let _ = rapier3d::rayon::ThreadPoolBuilder::new()
            .num_threads(threads.max(1))
            .thread_name(|i| format!("physics-{i}"))
            .build_global();
    });
}

/// Workspace.Gravity's default, in studs/s² (creator-docs Workspace.Gravity).
const DEFAULT_GRAVITY: f32 = 196.2;

/// Workspace.FallenPartsDestroyHeight's default (creator-docs).
const DEFAULT_FALLEN_HEIGHT: f32 = -500.0;

/// A part's volume, from its shape and Size: a truss, or a union or
/// MeshPart without saved geometry, is its box.
pub fn part_volume(dom: &Dom, part: InstanceId) -> f32 {
    match dom.vector3_property(part, "Size") {
        Some(s) => part_shape(dom, part).volume(s.max(glam::Vec3::splat(shape::MIN_PART_SIZE))),
        None => 0.0,
    }
}

/// Script requests physics carries out at its next step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    FireRocket(InstanceId),
    AbortRocket(InstanceId),
}

/// Which unanchored assemblies a world simulates (network ownership). An
/// assembly belongs to a model when one of its parts is inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authority {
    /// Every one but those of these models, which clients simulate and
    /// send (a server).
    Except(Vec<InstanceId>),
    /// Only those of these models (a client: its own character). With
    /// none, the world never steps and only answers queries.
    Only(Vec<InstanceId>),
}

impl Default for Authority {
    fn default() -> Self {
        Self::Except(Vec::new())
    }
}

impl Authority {
    /// Whether this world simulates the assembly holding `parts`.
    fn simulates(&self, dom: &Dom, parts: impl IntoIterator<Item = InstanceId>) -> bool {
        let (models, inside) = match self {
            Self::Except(models) => (models, false),
            Self::Only(models) => (models, true),
        };
        if models.is_empty() {
            return !inside;
        }
        let owned = parts.into_iter().any(|part| {
            let mut cur = dom.parent(part);
            while let Some(c) = cur {
                if models.contains(&c) {
                    return true;
                }
                cur = dom.parent(c);
            }
            false
        });
        owned == inside
    }
}

pub struct Physics {
    world: PhysicsWorld,
    props: Option<Props>,
    pub(crate) bodies: FxHashMap<RigidBodyHandle, Body>,
    /// Bodies with motors inside, so stepping motors needn't visit every
    /// body (a place with 79k bodies spent milliseconds a frame finding its
    /// two).
    motored: FxHashSet<RigidBodyHandle>,
    /// Unanchored bodies this world simulates, and those another machine
    /// does, so a step knows whether anything can move without visiting
    /// every body.
    moving: [usize; 2],
    parts: FxHashMap<InstanceId, Home>,
    /// Anchored parts on their own, with no joints and no velocity: most
    /// of a large place. Their colliders have no body at all, Rapier's
    /// (and PhysX's and Jolt's) static geometry: every body costs time each
    /// step, moving or not (84k fixed bodies took 1.4 ms a 240 Hz step),
    /// and every collider on a body is read again whenever one on it moves
    /// (267k colliders on one shared body made a script turning one
    /// anchored part cost 88 ms a step).
    statics: FxHashMap<InstanceId, Static>,
    /// An empty fixed body at the origin, which constraints and movers
    /// holding a static part are attached to, at the part's place.
    shared: RigidBodyHandle,
    /// Joints in the world and the parts they joined, so a changed joint's
    /// former parts are rebuilt too.
    joints: FxHashMap<InstanceId, [InstanceId; 2]>,
    hinges: FxHashMap<InstanceId, ImpulseJointHandle>,
    /// Journal position up to which changes have been applied.
    cursor: u64,
    unsettled: sync::settle::Unsettled,
    built: bool,
    pub(crate) fallen_height: f32,
    /// The last step's length, for advancing motors.
    last_dt: f64,
    /// Time carried over to the next step, less than one.
    pending: f64,
    /// The moving bodies' poses before the last step, which drawing
    /// interpolates from.
    before_last_step: FxHashMap<RigidBodyHandle, Pose>,
    /// The bodies awake when the last frame's steps began: those asleep
    /// after them are written once more, sleep having zeroed their
    /// velocities.
    awake: Vec<RigidBodyHandle>,
    /// The awake bodies' velocities before the step under way (linear,
    /// angular), to tell how fast each accelerates.
    before_step: Vec<(RigidBodyHandle, glam::Vec3, glam::Vec3)>,
    /// Bodies gravity alone moved over the last step, about to land if
    /// they land at all: only these pay for [`contacts::Mixer`].
    flying: FxHashSet<RigidBodyHandle>,
    /// Which part each collider belongs to.
    pub(crate) colliders: FxHashMap<ColliderHandle, InstanceId>,
    groups: CollisionGroups,
    /// Workspace.TouchesUseCollisionGroups: whether groups that don't
    /// collide also don't touch.
    pub(crate) touches_use_groups: bool,
    touch: contacts::Tracker,
    /// Mover instances seen in the world (some may have left it since).
    mover_ids: FxHashSet<InstanceId>,
    /// Movers acting now, in a stable order so forces add up the same
    /// way every run.
    movers: BTreeMap<InstanceId, Mover>,
    /// Bodies the movers pushed last step.
    forced: Vec<RigidBodyHandle>,
    /// Fired rockets, and whether each is within its target's radius.
    rockets: FxHashMap<InstanceId, bool>,
    /// Rockets that reached their target since the last call.
    reached: Vec<InstanceId>,
    constraint_ids: FxHashSet<InstanceId>,
    /// Movers and constraints to re-read at the next sync.
    dirty: FxHashSet<InstanceId>,
    attached: joints::attached::Attached,
    /// The constraints the last syncs re-read, for tests.
    #[cfg(test)]
    reread: Vec<InstanceId>,
    constraints: FxHashMap<InstanceId, BuiltConstraint>,
    /// The HingeConstraints among `constraints`, sorted: each step writes
    /// their `CurrentAngle`, in this order.
    hinge_constraints: Vec<InstanceId>,
    /// Driven revolute joints: RotateV, RotateP and HingeConstraints.
    drives: FxHashMap<InstanceId, Driven>,
    /// Bodies added or moved since the last step, whose colliders queries
    /// must put in place first (Rapier does that when it steps).
    stale: FxHashSet<RigidBodyHandle>,
    /// The same for [`Static`]s added or moved.
    stale_statics: FxHashSet<InstanceId>,
    /// Every part's `Size` box, for the queries that find parts by it.
    bounds: query::bounds::PartBounds,
    authority: Authority,
    /// Parts whose assemblies change hands at the next sync.
    handed_over: Vec<InstanceId>,
    saved: saved::SavedShapes,
    terrain: terrain::Terrain,
    /// Terrain colliders were added since the last query.
    terrain_placed: bool,
    /// Counts changes to what queries see: parts added, removed, moved or
    /// changed, bodies stepped, terrain built.
    generation: u64,
    /// Humanoids seen in Workspace (some may have left it since).
    humanoid_ids: FxHashSet<InstanceId>,
    /// TrussParts seen in Workspace, which Humanoids climb.
    trusses: FxHashSet<InstanceId>,
    /// The Humanoids this world drives, in id order.
    controllers: BTreeMap<InstanceId, humanoid::Controller>,
    /// The server's assemblies' updates, on a client.
    playback: remote::Playback,
}

impl std::fmt::Debug for Physics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Physics")
            .field("parts", &self.parts.len())
            .field("bodies", &self.bodies.len())
            .field("cursor", &self.cursor)
            .finish_non_exhaustive()
    }
}

impl Default for Physics {
    fn default() -> Self {
        Self::new()
    }
}

impl Physics {
    pub fn new() -> Self {
        let mut world = PhysicsWorld::new();
        world.gravity = Vector::new(0.0, -DEFAULT_GRAVITY, 0.0);
        world.integration_parameters.dt = (1.0 / STEP_HZ) as f32;
        // No continuous collision: Roblox lets a fast part pass through a
        // thin one, and Rapier's sweeps cost ~19 ms a frame on 60k-part
        // places.
        world.integration_parameters.max_ccd_substeps = 0;
        // At the origin, so a static part's place on it is its CFrame.
        let shared = world.insert_body(RigidBodyBuilder::fixed());
        Self {
            world,
            props: None,
            bodies: FxHashMap::default(),
            motored: FxHashSet::default(),
            moving: [0; 2],
            parts: FxHashMap::default(),
            statics: FxHashMap::default(),
            shared,
            joints: FxHashMap::default(),
            hinges: FxHashMap::default(),
            cursor: 0,
            unsettled: sync::settle::Unsettled::default(),
            built: false,
            fallen_height: DEFAULT_FALLEN_HEIGHT,
            last_dt: 0.0,
            pending: 0.0,
            before_last_step: FxHashMap::default(),
            awake: Vec::new(),
            before_step: Vec::new(),
            flying: FxHashSet::default(),
            colliders: FxHashMap::default(),
            groups: CollisionGroups::default(),
            touches_use_groups: false,
            touch: contacts::Tracker::default(),
            mover_ids: FxHashSet::default(),
            movers: BTreeMap::new(),
            forced: Vec::new(),
            rockets: FxHashMap::default(),
            reached: Vec::new(),
            constraint_ids: FxHashSet::default(),
            dirty: FxHashSet::default(),
            attached: Default::default(),
            #[cfg(test)]
            reread: Vec::new(),
            generation: 0,
            constraints: FxHashMap::default(),
            hinge_constraints: Vec::new(),
            drives: FxHashMap::default(),
            stale: FxHashSet::default(),
            stale_statics: FxHashSet::default(),
            bounds: query::bounds::PartBounds::default(),
            authority: Authority::default(),
            handed_over: Vec::new(),
            saved: saved::SavedShapes::default(),
            terrain: terrain::Terrain::default(),
            terrain_placed: false,
            humanoid_ids: FxHashSet::default(),
            trusses: FxHashSet::default(),
            controllers: BTreeMap::new(),
            playback: remote::Playback::default(),
        }
    }

    /// A world that simulates only what `authority` gives it.
    pub fn with_authority(authority: Authority) -> Self {
        Self {
            authority,
            ..Self::new()
        }
    }

    /// Hand assemblies over: those that change hands are rebuilt at the
    /// next sync, simulated here or following the DOM.
    pub fn set_authority(&mut self, dom: &Dom, authority: Authority) {
        if authority == self.authority {
            return;
        }
        self.authority = authority;
        for body in self.bodies.values() {
            let parts = body.asm.members.iter().map(|m| m.part);
            if !body.asm.anchored && self.authority.simulates(dom, parts) == body.remote {
                self.handed_over.push(body.asm.root());
            }
        }
    }

    /// Whether stepping can move anything: an assembly this world
    /// simulates, or (on a server) one a client moves, for its touches.
    fn moves_anything(&self) -> bool {
        let touches_remote = matches!(self.authority, Authority::Except(_));
        let [local, remote] = self.moving;
        local > 0 || (touches_remote && remote > 0) || self.playback.any()
    }

    /// Whether this world simulates `part`'s assembly (`false` for an
    /// anchored one or one outside the world).
    pub fn simulates(&self, part: InstanceId) -> bool {
        self.body_of(part)
            .is_some_and(|h| !self.bodies[&h].asm.anchored && !self.bodies[&h].remote)
    }

    /// The body of `part`'s assembly; a static part has none.
    pub(crate) fn body_of(&self, part: InstanceId) -> Option<RigidBodyHandle> {
        match self.parts.get(&part)? {
            Home::Body(h) => Some(*h),
            Home::Static => None,
        }
    }

    /// Whether the world has been built from the DOM (by a sync).
    pub fn is_built(&self) -> bool {
        self.built
    }

    pub fn command(&mut self, command: Command) {
        match command {
            Command::FireRocket(id) => {
                self.rockets.insert(id, false);
                self.mover_ids.insert(id);
                self.dirty.insert(id);
            }
            Command::AbortRocket(id) => {
                self.rockets.remove(&id);
                self.dirty.insert(id);
            }
        }
    }

    /// Rockets that reached their target since the last call, for their
    /// ReachedTarget events.
    pub fn take_reached(&mut self) -> Vec<InstanceId> {
        std::mem::take(&mut self.reached)
    }

    /// Touched and TouchEnded events since the last call, in order.
    pub fn take_touches(&mut self) -> Vec<contacts::Touch> {
        self.touch.take()
    }

    /// A number that changes whenever what queries see may have: compare
    /// it after a [`Physics::sync`] to know whether a query's answer can
    /// be reused.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn part_count(&self) -> usize {
        self.parts.len()
    }

    /// Simulated (unanchored) assemblies.
    pub fn body_count(&self) -> usize {
        self.moving.iter().sum()
    }

    /// The parts that move with `part` (its assembly's, `part` included):
    /// physics journals only the root's CFrame when a whole assembly moves
    /// ([`Physics::write_back`]). Empty for a part on its own.
    pub fn moved_with(&self, part: InstanceId) -> impl Iterator<Item = InstanceId> + '_ {
        let members = match self.parts.get(&part) {
            Some(Home::Body(h)) => self.bodies[h].asm.members.as_slice(),
            _ => &[],
        };
        members.iter().map(|m| m.part)
    }

    /// The root part of the assembly `part` is simulated in.
    pub fn root_of(&self, part: InstanceId) -> Option<InstanceId> {
        match self.parts.get(&part)? {
            Home::Body(h) => Some(self.bodies[h].asm.root()),
            Home::Static => Some(part),
        }
    }

    /// `BasePart:GetMass()`: volume × density (observed in Studio), a union's
    /// or MeshPart's volume being the one saved with its hulls.
    pub fn part_mass(&self, dom: &Dom, part: InstanceId) -> f32 {
        let volume = dom
            .vector3_property(part, "Size")
            .and_then(|size| self.saved.mass(dom, part, size))
            .map_or_else(|| part_volume(dom, part), |m| m.volume);
        volume * materials::of_part(dom, part).density
    }

    /// Where `part`'s own center of mass is, in the world.
    pub(crate) fn part_center(&self, dom: &Dom, part: InstanceId, cf: CFrame) -> glam::Vec3 {
        let center = dom
            .vector3_property(part, "Size")
            .and_then(|size| self.saved.mass(dom, part, size))
            .map_or(glam::Vec3::ZERO, |m| m.center);
        cf.point_to_world_space(rook_types::Vector3(center)).0
    }

    /// Bring the world up to date with the DOM: on the first call, add
    /// every Workspace part; after that, apply the journal since last time.
    /// Parts are snapped to where their joints hold them.
    pub fn sync(&mut self, dom: &mut Dom) {
        profiling::scope!("Physics::sync");
        if self.props.is_none() {
            self.props = Props::resolve(dom);
        }
        let Some(props) = self.props.take() else {
            return;
        };
        rook_types::gauge!(PhysicsParts, self.parts.len());
        if !self.built {
            self.handed_over.clear();
            self.build(dom, &props);
        } else {
            self.apply_changes(dom, &props);
        }
        let handed_over = std::mem::take(&mut self.handed_over);
        if !handed_over.is_empty() {
            self.rebuild(dom, &props, handed_over);
        }
        self.refresh_terrain(dom);
        let dirty = std::mem::take(&mut self.dirty);
        self.refresh_constraints(dom, &dirty);
        self.refresh_movers(dom, &dirty);
        self.bounds.place_added(dom);
        self.unsettled.clear();
        // Our own snapping writes aren't changes to apply next time.
        self.cursor = dom.journal().end();
        self.props = Some(props);
    }
}

#[cfg(test)]
mod tests;
