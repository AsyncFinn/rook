//! Roblox physics semantics over Rapier.
//!
//! [`Physics`] mirrors Workspace's parts into a Rapier world, keeps up with
//! scripts through the DOM's change journal, steps it and writes simulated
//! parts' CFrames and velocities back. Rigidly joined parts form one body
//! ([`assembly`]); [`joints`] covers joints, constraints and movers;
//! [`contacts`] materials, collision groups and Touched; [`query`] scripts'
//! scene queries, which a client's world only answers
//! ([`Physics::for_queries`]). Findings are in `docs/behavior/physics.md` and
//! `docs/behavior/collision.md`.

pub mod assembly;
pub mod contacts;
mod convert;
mod explosion;
pub mod joints;
pub mod query;
mod shape;
mod step;
mod sync;

use std::collections::BTreeMap;

use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

use rapier3d::prelude::*;
use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::PropertyId;
use rook_types::{CFrame, Value, Vector3};

pub use shape::{is_part, part_shape, workspace_parts};

pub use explosion::Blast;
pub use query::{CastShape, Filter, Hit, Region};

use assembly::bodies::Body;
use assembly::{Assembly, is_massless};
use contacts::groups::CollisionGroups;
use convert::{cframe_of, pose_of, to_glam, to_rapier};
use joints::constraints::{Drive, Spec, apply_drive, hinge_constraint_drive, revolute_angle};
use joints::drives::{BuiltConstraint, DriveSource, Driven};
use joints::movers::Mover;
use joints::{HingeInput, Joint, JointKind, active_joint, endpoints, is_joint};
use sync::props::Props;

use contacts::{groups, materials};
use joints::{constraints, movers};
use shape::{terrain, unions};

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

/// A part's volume, from its shape and Size. MeshParts, unions and
/// trusses count as blocks until meshes exist.
pub fn part_volume(dom: &Dom, part: InstanceId) -> f32 {
    match dom.property_by_name(part, "Size") {
        Some(Value::Vector3(s)) => {
            part_shape(dom, part).volume(s.0.max(glam::Vec3::splat(shape::MIN_PART_SIZE)))
        }
        _ => 0.0,
    }
}

/// `BasePart:GetMass()`: volume × density (observed in Studio).
pub fn part_mass(dom: &Dom, part: InstanceId) -> f32 {
    part_volume(dom, part) * materials::of_part(dom, part).density
}

/// Move the velocities files save (the deprecated `Velocity` and
/// `RotVelocity`) into `AssemblyLinearVelocity` and
/// `AssemblyAngularVelocity`, which aren't saved and are the ones physics
/// and scripts use. Scripts see `Velocity` and `RotVelocity` as views of
/// them (observed: setting either sets the assembly's velocity at once).
pub fn adopt_saved_velocities(dom: &mut Dom) {
    let db = dom.database();
    let Some(base) = db.class_by_name("BasePart").map(|c| c.id) else {
        return;
    };
    let p = |name| db.find_canonical_property(base, name).map(|p| p.id);
    let pairs = [
        (p("Velocity"), p("AssemblyLinearVelocity")),
        (p("RotVelocity"), p("AssemblyAngularVelocity")),
    ];
    for id in dom.descendants(dom.root()) {
        if !dom.is_a(id, "BasePart") {
            continue;
        }
        for (saved, used) in pairs {
            let (Some(saved), Some(used)) = (saved, used) else {
                continue;
            };
            if let Some(Value::Vector3(v)) = dom.property(id, saved)
                && v.0 != glam::Vec3::ZERO
            {
                rook_dom::ignore_stale(dom.set_property(id, used, Value::Vector3(v)));
            }
        }
    }
}

/// Script requests physics carries out at its next step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// `RocketPropulsion:Fire()`.
    FireRocket(InstanceId),
    /// `RocketPropulsion:Abort()`.
    AbortRocket(InstanceId),
}

/// A DataModel's physics world.
pub struct Physics {
    world: PhysicsWorld,
    props: Option<Props>,
    pub(crate) bodies: HashMap<RigidBodyHandle, Body>,
    /// Bodies with motors inside, so stepping motors needn't visit every
    /// body (a place with 79k bodies spent milliseconds a frame finding its
    /// two).
    motored: HashSet<RigidBodyHandle>,
    parts: HashMap<InstanceId, RigidBodyHandle>,
    /// Joints in the world and the parts they joined, so a changed joint's
    /// former parts are rebuilt too.
    joints: HashMap<InstanceId, [InstanceId; 2]>,
    hinges: HashMap<InstanceId, ImpulseJointHandle>,
    /// Journal position up to which changes have been applied.
    cursor: u64,
    built: bool,
    pub(crate) fallen_height: f32,
    /// The last step's length, for advancing motors.
    last_dt: f64,
    /// Which part each collider belongs to.
    pub(crate) colliders: HashMap<ColliderHandle, InstanceId>,
    groups: CollisionGroups,
    /// Workspace.TouchesUseCollisionGroups: whether groups that don't
    /// collide also don't touch.
    pub(crate) touches_use_groups: bool,
    touch: contacts::Tracker,
    /// Mover instances seen in the world (some may have left it since).
    mover_ids: HashSet<InstanceId>,
    /// Movers acting now, in a stable order so forces add up the same
    /// way every run.
    movers: BTreeMap<InstanceId, Mover>,
    /// Bodies the movers pushed last step.
    forced: Vec<RigidBodyHandle>,
    /// Fired rockets, and whether each is within its target's radius.
    rockets: HashMap<InstanceId, bool>,
    /// Rockets that reached their target since the last call.
    reached: Vec<InstanceId>,
    /// Attachment constraints seen in the world.
    constraint_ids: HashSet<InstanceId>,
    /// Movers and constraints to re-read at the next sync, or all of them.
    dirty: HashSet<InstanceId>,
    all_dirty: bool,
    constraints: HashMap<InstanceId, BuiltConstraint>,
    /// Driven revolute joints: RotateV, RotateP and HingeConstraints.
    drives: HashMap<InstanceId, Driven>,
    /// Bodies added or moved since the last step, whose colliders queries
    /// must put in place first (Rapier does that when it steps).
    stale: HashSet<RigidBodyHandle>,
    /// The world only answers queries; it never steps (clients).
    query_only: bool,
    unions: unions::Unions,
    terrain: terrain::Terrain,
    /// Terrain colliders were added since the last query.
    terrain_placed: bool,
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
        Self {
            world,
            props: None,
            bodies: HashMap::default(),
            motored: HashSet::default(),
            parts: HashMap::default(),
            joints: HashMap::default(),
            hinges: HashMap::default(),
            cursor: 0,
            built: false,
            fallen_height: DEFAULT_FALLEN_HEIGHT,
            last_dt: 0.0,
            colliders: HashMap::default(),
            groups: CollisionGroups::default(),
            touches_use_groups: false,
            touch: contacts::Tracker::default(),
            mover_ids: HashSet::default(),
            movers: BTreeMap::new(),
            forced: Vec::new(),
            rockets: HashMap::default(),
            reached: Vec::new(),
            constraint_ids: HashSet::default(),
            dirty: HashSet::default(),
            all_dirty: false,
            constraints: HashMap::default(),
            drives: HashMap::default(),
            stale: HashSet::default(),
            query_only: false,
            unions: unions::Unions::default(),
            terrain: terrain::Terrain::default(),
            terrain_placed: false,
        }
    }

    /// A world that only answers queries, for a DataModel that doesn't
    /// simulate (a client: the server simulates every part until network
    /// ownership exists). Its parts stay where replication puts them.
    pub fn for_queries() -> Self {
        Self {
            query_only: true,
            ..Self::new()
        }
    }

    /// Whether the world has been built from the DOM (by a sync).
    pub fn is_built(&self) -> bool {
        self.built
    }

    /// Carry out a script's request.
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

    /// Parts in the world.
    pub fn part_count(&self) -> usize {
        self.parts.len()
    }

    /// Simulated (unanchored) assemblies.
    pub fn body_count(&self) -> usize {
        self.bodies.values().filter(|b| !b.asm.anchored).count()
    }

    /// The root part of the assembly `part` is simulated in.
    pub fn root_of(&self, part: InstanceId) -> Option<InstanceId> {
        Some(self.bodies[self.parts.get(&part)?].asm.root())
    }

    /// A union's collision geometry, loaded (or `None`: it collides as its
    /// box again). It takes effect at the next sync.
    pub fn set_union_collision(
        &mut self,
        part: InstanceId,
        collision: Option<std::sync::Arc<rook_types::UnionCollision>>,
    ) {
        self.unions.set(part, collision);
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
        if !self.built {
            self.build(dom, &props);
        } else {
            self.apply_changes(dom, &props);
        }
        self.reshape_unions(dom, &props);
        self.refresh_terrain(dom);
        let all = std::mem::take(&mut self.all_dirty);
        let dirty = std::mem::take(&mut self.dirty);
        self.refresh_constraints(dom, all, &dirty);
        self.refresh_movers(dom, all, &dirty);
        // Our own snapping writes aren't changes to apply next time.
        self.cursor = dom.journal().end();
        self.props = Some(props);
    }
}

#[cfg(test)]
mod tests;
