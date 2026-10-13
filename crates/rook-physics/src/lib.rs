//! Roblox physics semantics over Rapier.
//!
//! [`Physics`] mirrors Workspace's parts into a Rapier world. It reads the
//! DOM's change journal to keep up with scripts, steps the simulation, and
//! writes simulated parts' CFrame and velocities back into the DOM.
//!
//! Parts joined by rigid joints form an assembly ([`assembly`]), which is
//! one Rapier body with a collider per part; anchored assemblies are fixed
//! bodies. Hinge joints connect bodies with Rapier revolute joints.
//! [`joints`], [`assembly`] and [`surfaces`] work on the DOM alone, so
//! scripts can use them synchronously.
//!
//! Parts simulate with their material's properties ([`materials`]), in
//! their collision group ([`groups`]), and their contacts become Touched
//! and TouchEnded ([`Physics::take_touches`]).
//!
//! BodyMovers and force constraints push bodies every step ([`movers`]);
//! attachment constraints become Rapier joints ([`constraints`]), and
//! RotateV, RotateP and HingeConstraint actuators drive their joints'
//! motors.
//!
//! Scripts' scene queries (rays, shape casts, regions, overlaps) run on
//! the same world ([`query`]); each brings it up to date with the DOM
//! first. A client's world only answers queries ([`Physics::for_queries`]).

pub mod assembly;
mod constraints;
mod contacts;
mod convert;
pub mod groups;
pub mod joints;
pub mod materials;
mod movers;
pub mod query;
mod read;
mod shape;
pub mod surfaces;

use std::collections::{BTreeMap, HashMap, HashSet};

use rapier3d::prelude::*;
use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::PropertyId;
use rook_types::{CFrame, Value, Vector3};

pub use rook_types::PartShape;
pub use shape::{is_part, part_shape, workspace_parts};

pub use contacts::Touch;
pub use query::{CastShape, Filter, Hit, Region};

use assembly::{Assembly, is_massless};
use constraints::{Drive, Spec, apply_drive, hinge_constraint_drive, revolute_angle};
use convert::{cframe_of, pose_of, to_glam, to_rapier};
use groups::CollisionGroups;
use joints::{HingeInput, Joint, JointKind, active_joint, endpoints, is_joint};
use movers::Mover;

/// Roblox simulates at 240 Hz (creator-docs "Adaptive timestepping": 240,
/// 120 or 60 Hz per assembly; Rook always uses 240, an Approximation).
pub const STEP_HZ: f64 = 240.0;

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
                let _ = dom.set_property(id, used, Value::Vector3(v));
            }
        }
    }
}

/// The physics properties [`Physics`] watches, resolved once.
#[derive(Debug)]
struct Props {
    cframe: PropertyId,
    size: PropertyId,
    anchored: PropertyId,
    can_collide: PropertyId,
    massless: PropertyId,
    root_priority: PropertyId,
    material: PropertyId,
    custom_properties: PropertyId,
    collision_group: PropertyId,
    can_touch: PropertyId,
    shape: Option<PropertyId>,
    linear_velocity: PropertyId,
    angular_velocity: PropertyId,
    gravity: PropertyId,
    fallen_height: PropertyId,
    group_data: PropertyId,
    touches_use_groups: PropertyId,
    /// Each face's ParamA, ParamB and SurfaceInput, which drive RotateV
    /// and RotateP.
    surface_inputs: Vec<PropertyId>,
}

impl Props {
    fn resolve(dom: &Dom) -> Option<Self> {
        let db = dom.database();
        let base = db.class_by_name("BasePart")?.id;
        let part = db.class_by_name("Part")?.id;
        let ws = db.class_by_name("Workspace")?.id;
        let p = |class, name: &str| db.find_canonical_property(class, name).map(|p| p.id);
        Some(Self {
            cframe: p(base, "CFrame")?,
            size: p(base, "Size")?,
            anchored: p(base, "Anchored")?,
            can_collide: p(base, "CanCollide")?,
            massless: p(base, "Massless")?,
            root_priority: p(base, "RootPriority")?,
            material: p(base, "Material")?,
            custom_properties: p(base, "CustomPhysicalProperties")?,
            collision_group: p(base, "CollisionGroup")?,
            can_touch: p(base, "CanTouch")?,
            shape: p(part, "Shape"),
            linear_velocity: p(base, "AssemblyLinearVelocity")?,
            angular_velocity: p(base, "AssemblyAngularVelocity")?,
            gravity: p(ws, "Gravity")?,
            fallen_height: p(ws, "FallenPartsDestroyHeight")?,
            group_data: p(ws, "CollisionGroupData")?,
            touches_use_groups: p(ws, "TouchesUseCollisionGroups")?,
            surface_inputs: ["Top", "Bottom", "Left", "Right", "Front", "Back"]
                .iter()
                .flat_map(|face| {
                    ["ParamA", "ParamB", "SurfaceInput"]
                        .map(|input| p(base, &format!("{face}{input}")))
                })
                .collect::<Option<_>>()?,
        })
    }

    /// Changes that alter which bodies and colliders exist, or which
    /// colliders interact (`CanCollide` and `CollisionGroup` change contact
    /// pairs, and with them touch events, so they rebuild too).
    fn is_structural(&self, p: PropertyId) -> bool {
        p == self.size
            || p == self.anchored
            || p == self.can_collide
            || p == self.massless
            || p == self.root_priority
            || p == self.collision_group
            || Some(p) == self.shape
    }

    /// Changes to a collider's mass, friction, elasticity or touchability
    /// alone, which update it in place.
    fn is_surface(&self, p: PropertyId) -> bool {
        p == self.material || p == self.custom_properties || p == self.can_touch
    }

    /// Workspace properties physics reads.
    fn is_workspace(&self, p: PropertyId) -> bool {
        p == self.gravity
            || p == self.fallen_height
            || p == self.group_data
            || p == self.touches_use_groups
    }

    /// Changes that move a body.
    fn is_pose(&self, p: PropertyId) -> bool {
        p == self.cframe || p == self.linear_velocity || p == self.angular_velocity
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

/// What sets a driven revolute joint's motor.
#[derive(Debug, Clone, Copy)]
enum DriveSource {
    /// RotateV or RotateP.
    Surface(HingeInput),
    /// A HingeConstraint's actuator.
    Actuator(Drive),
}

/// A revolute joint with a motor.
#[derive(Debug)]
struct Driven {
    joint: ImpulseJointHandle,
    source: DriveSource,
    /// How long a Sin input has run.
    time: f64,
    /// RotateP's angle, which steps toward its input; `None` until the
    /// joint's angle is first read.
    angle: Option<f32>,
}

/// An attachment constraint in the world.
#[derive(Debug)]
struct BuiltConstraint {
    spec: Spec,
    bodies: [RigidBodyHandle; 2],
    offsets: [CFrame; 2],
    joint: ImpulseJointHandle,
}

/// One assembly in the Rapier world.
#[derive(Debug)]
struct Body {
    asm: Assembly,
    /// Per member: its collider and the collider's pose in the part.
    colliders: Vec<Option<(ColliderHandle, Pose)>>,
    /// Motors inside the assembly, stepped every frame.
    motors: Vec<Joint>,
}

/// A DataModel's physics world.
pub struct Physics {
    world: PhysicsWorld,
    props: Option<Props>,
    bodies: HashMap<RigidBodyHandle, Body>,
    parts: HashMap<InstanceId, RigidBodyHandle>,
    /// Joints in the world and the parts they joined, so a changed joint's
    /// former parts are rebuilt too.
    joints: HashMap<InstanceId, [InstanceId; 2]>,
    hinges: HashMap<InstanceId, ImpulseJointHandle>,
    /// Journal position up to which changes have been applied.
    cursor: u64,
    built: bool,
    fallen_height: f32,
    /// The last step's length, for advancing motors.
    last_dt: f64,
    /// Which part each collider belongs to.
    colliders: HashMap<ColliderHandle, InstanceId>,
    groups: CollisionGroups,
    /// Workspace.TouchesUseCollisionGroups: whether groups that don't
    /// collide also don't touch.
    touches_use_groups: bool,
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

/// A part's collider mass and material. Massless parts add no mass unless
/// they're the root (creator-docs BasePart.Massless).
fn collider_surface(dom: &Dom, part: InstanceId, is_root: bool) -> (f32, materials::Properties) {
    let mass = if !is_root && is_massless(dom, part) {
        0.0
    } else {
        part_mass(dom, part)
    };
    (mass, materials::of_part(dom, part))
}

/// Rapier's revolute joints turn about their frames' X axis; a Roblox
/// hinge turns about its C0/C1 Z axis.
fn hinge_pose(cf: CFrame) -> Pose {
    pose_of(&cf) * Pose::from_rotation(Rotation::from_rotation_y(-std::f32::consts::FRAC_PI_2))
}

impl Physics {
    pub fn new() -> Self {
        let mut world = PhysicsWorld::new();
        world.gravity = Vector::new(0.0, -DEFAULT_GRAVITY, 0.0);
        world.integration_parameters.dt = (1.0 / STEP_HZ) as f32;
        Self {
            world,
            props: None,
            bodies: HashMap::new(),
            parts: HashMap::new(),
            joints: HashMap::new(),
            hinges: HashMap::new(),
            cursor: 0,
            built: false,
            fallen_height: DEFAULT_FALLEN_HEIGHT,
            last_dt: 0.0,
            colliders: HashMap::new(),
            groups: CollisionGroups::default(),
            touches_use_groups: false,
            touch: contacts::Tracker::default(),
            mover_ids: HashSet::new(),
            movers: BTreeMap::new(),
            forced: Vec::new(),
            rockets: HashMap::new(),
            reached: Vec::new(),
            constraint_ids: HashSet::new(),
            dirty: HashSet::new(),
            all_dirty: false,
            constraints: HashMap::new(),
            drives: HashMap::new(),
            stale: HashSet::new(),
            query_only: false,
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
    pub fn take_touches(&mut self) -> Vec<Touch> {
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
        let all = std::mem::take(&mut self.all_dirty);
        let dirty = std::mem::take(&mut self.dirty);
        self.refresh_constraints(dom, all, &dirty);
        self.refresh_movers(dom, all, &dirty);
        // Our own snapping writes aren't changes to apply next time.
        self.cursor = dom.journal().end();
        self.props = Some(props);
    }

    fn build(&mut self, dom: &mut Dom, props: &Props) {
        profiling::scope!("Physics::build");
        self.read_workspace(dom, props);
        let parts = workspace_parts(dom);
        self.rebuild(dom, props, parts);
        if let Some(ws) = dom.find_service("Workspace") {
            for id in dom.descendants(ws) {
                self.notice(dom, id);
            }
        }
        self.built = true;
    }

    fn apply_changes(&mut self, dom: &mut Dom, props: &Props) {
        profiling::scope!("Physics::apply_changes");
        let mut structural: Vec<InstanceId> = Vec::new();
        let mut changed_joints: Vec<InstanceId> = Vec::new();
        let mut moved_joints: Vec<InstanceId> = Vec::new();
        let mut posed: Vec<InstanceId> = Vec::new();
        let mut surfaces: Vec<InstanceId> = Vec::new();
        let mut materials: Vec<InstanceId> = Vec::new();
        let mut attachments: Vec<InstanceId> = Vec::new();
        let mut workspace_changed = false;
        let db = dom.database();
        for change in dom.journal().since(self.cursor) {
            match change {
                Change::Property { id, property } => {
                    let p = *property;
                    if props.is_structural(p) {
                        structural.push(*id);
                    } else if props.is_surface(p) {
                        materials.push(*id);
                    } else if props.is_pose(p) {
                        posed.push(*id);
                    } else if props.is_workspace(p) {
                        workspace_changed = true;
                    } else if self.joints.contains_key(id) || is_joint(dom, *id) {
                        match db.property(p).name {
                            "Part0" | "Part1" | "Enabled" => changed_joints.push(*id),
                            "C0" | "C1" | "Transform" | "CurrentAngle" => moved_joints.push(*id),
                            _ => {}
                        }
                    } else if self.constraint_ids.contains(id) || self.mover_ids.contains(id) {
                        self.dirty.insert(*id);
                    } else if props.surface_inputs.contains(&p) {
                        if !self.drives.is_empty() && self.parts.contains_key(id) {
                            surfaces.push(*id);
                        }
                    } else if !(self.constraint_ids.is_empty() && self.mover_ids.is_empty())
                        && dom.class_name(*id) == "Attachment"
                    {
                        attachments.push(*id);
                    }
                }
                Change::Reparented { subtree, .. } => {
                    for &i in subtree {
                        if self.parts.contains_key(&i) || is_part(dom, i) {
                            structural.push(i);
                        } else if self.joints.contains_key(&i) || is_joint(dom, i) {
                            changed_joints.push(i);
                        } else if dom.class_name(i) == "Attachment" {
                            attachments.push(i);
                        } else {
                            self.notice(dom, i);
                        }
                    }
                }
                _ => {}
            }
        }
        if workspace_changed {
            self.read_workspace(dom, props);
        }
        // A hinge's frames are fixed when it's created, so moving one
        // recreates it.
        moved_joints.retain(|j| {
            let hinge = self.hinges.contains_key(j);
            if hinge {
                changed_joints.push(*j);
            }
            !hinge
        });
        for j in changed_joints {
            if let Some(old) = self.joints.get(&j) {
                structural.extend(old);
            }
            structural.extend(endpoints(dom, j).into_iter().flatten());
        }
        for att in attachments {
            for &(holder, _) in dom.referrers(att) {
                if self.constraint_ids.contains(&holder) || self.mover_ids.contains(&holder) {
                    self.dirty.insert(holder);
                }
            }
        }
        let rebuilt = if structural.is_empty() {
            HashSet::new()
        } else {
            // Bodies the movers pushed and constraints joined may be gone.
            self.all_dirty = true;
            self.rebuild(dom, props, structural)
        };
        for part in surfaces {
            for j in joints::part_joints(dom, part) {
                if let (Some(d), Some(input)) =
                    (self.drives.get_mut(&j.id), joints::hinge_input(dom, &j))
                {
                    d.source = DriveSource::Surface(input);
                }
            }
        }
        let mut refreshed = HashSet::new();
        for part in materials {
            // A rebuilt body's colliders already read the new values.
            if let Some(&h) = self.parts.get(&part)
                && !rebuilt.contains(&h)
                && refreshed.insert(part)
            {
                self.refresh_surface(dom, props, h, part);
            }
        }

        let mut done = rebuilt.clone();
        for j in moved_joints {
            let Some(&[p0, _]) = self.joints.get(&j) else {
                continue;
            };
            if let Some(&h) = self.parts.get(&p0)
                && done.insert(h)
            {
                self.refresh_offsets(dom, h);
            }
        }
        let mut posed_bodies = rebuilt;
        for part in posed {
            if let Some(&h) = self.parts.get(&part)
                && posed_bodies.insert(h)
            {
                self.teleport(dom, props, h, part);
            }
        }
    }

    fn read_workspace(&mut self, dom: &Dom, props: &Props) {
        let Some(ws) = dom.find_service("Workspace") else {
            return;
        };
        if let Some(Value::Float32(g)) = dom.property(ws, props.gravity) {
            self.world.gravity = Vector::new(0.0, -g, 0.0);
        }
        if let Some(Value::Float32(h)) = dom.property(ws, props.fallen_height) {
            self.fallen_height = h;
        }
        let groups = CollisionGroups::read(dom);
        let touches_use_groups =
            dom.property(ws, props.touches_use_groups) == Some(Value::Bool(true));
        if groups != self.groups || touches_use_groups != self.touches_use_groups {
            self.groups = groups;
            self.touches_use_groups = touches_use_groups;
            let colliders: Vec<_> = self.colliders.iter().map(|(h, p)| (*h, *p)).collect();
            for (handle, part) in colliders {
                let (solver, collision) = self.interaction_groups(dom, part);
                if let Some(c) = self.world.colliders.get_mut(handle) {
                    c.set_solver_groups(solver);
                    c.set_collision_groups(collision);
                }
            }
        }
    }

    /// Rebuild the assemblies of `seeds` and of every part that shared an
    /// assembly with one. Returns the new bodies.
    fn rebuild(
        &mut self,
        dom: &mut Dom,
        props: &Props,
        seeds: Vec<InstanceId>,
    ) -> HashSet<RigidBodyHandle> {
        profiling::scope!("Physics::rebuild", &format!("{} seeds", seeds.len()));
        // A part's neighbours may change assembly with it (anchoring a part
        // splits its assembly; unanchoring one joins a neighbour's).
        let mut seeds_and_neighbours = Vec::with_capacity(seeds.len());
        for part in seeds {
            seeds_and_neighbours.push(part);
            seeds_and_neighbours
                .extend(joints::part_joints(dom, part).iter().map(|j| j.other(part)));
        }
        let mut todo = Vec::with_capacity(seeds_and_neighbours.len());
        for part in seeds_and_neighbours {
            if let Some(&h) = self.parts.get(&part)
                && let Some(body) = self.remove_body(h)
            {
                todo.extend(body.asm.members.iter().map(|m| m.part));
            }
            todo.push(part);
        }
        let mut added = HashSet::new();
        let mut hinges = Vec::new();
        let mut clusters = assembly::Clusters::default();
        for part in todo {
            if self.parts.contains_key(&part) {
                continue;
            }
            let asm = {
                profiling::scope!("Assembly::of");
                Assembly::of_with(dom, part, &mut clusters)
            };
            if let Some(asm) = asm {
                hinges.extend(asm.hinges.iter().map(|h| h.id));
                added.insert(self.add(dom, props, asm));
            }
        }
        for joint in hinges {
            if !self.hinges.contains_key(&joint) {
                self.add_hinge(dom, joint);
            }
        }
        added
    }

    fn remove_body(&mut self, h: RigidBodyHandle) -> Option<Body> {
        profiling::scope!("Physics::remove_body");
        let body = self.bodies.remove(&h)?;
        // Removing the Rapier body removes its colliders and joints.
        self.world.remove_body(h);
        for (m, c) in body.asm.members.iter().zip(&body.colliders) {
            self.parts.remove(&m.part);
            self.touch.forget(m.part);
            if let Some((collider, _)) = c {
                self.colliders.remove(collider);
                self.touch.retire(*collider, m.part);
            }
        }
        for j in body.asm.joints() {
            self.joints.remove(&j.id);
        }
        for j in &body.asm.hinges {
            self.joints.remove(&j.id);
            self.hinges.remove(&j.id);
            self.drives.remove(&j.id);
        }
        Some(body)
    }

    fn add(&mut self, dom: &mut Dom, props: &Props, asm: Assembly) -> RigidBodyHandle {
        profiling::scope!("Physics::add");
        let root = asm.root();
        let root_cf = match dom.property(root, props.cframe) {
            Some(Value::CFrame(cf)) => cf,
            _ => CFrame::IDENTITY,
        };
        let vector = |p| match dom.property(root, p) {
            Some(Value::Vector3(v)) => to_rapier(v.0),
            _ => Vector::ZERO,
        };
        let builder = if asm.anchored {
            // An anchored part keeps its velocity and carries what touches
            // it along: a conveyor (observed).
            RigidBodyBuilder::fixed()
                .user_data(contacts::pack_velocity(vector(props.linear_velocity)))
        } else {
            // Rapier caps spin at a quarter turn per step (for CCD, which
            // Rook doesn't use); Roblox spins a RotateV rotor at 1,550
            // rad/s (observed).
            RigidBodyBuilder::dynamic()
                .linvel(vector(props.linear_velocity))
                .angvel(vector(props.angular_velocity))
                .allow_fast_rotation(true)
        };
        let handle = self.world.insert_body(builder.pose(pose_of(&root_cf)));
        self.stale.insert(handle);
        let mut colliders = Vec::with_capacity(asm.members.len());
        for (i, m) in asm.members.iter().enumerate() {
            self.parts.insert(m.part, handle);
            colliders.push(self.add_collider(dom, props, handle, m.part, m.offset, i == 0));
        }
        for j in asm.joints().chain(&asm.hinges) {
            self.joints.insert(j.id, [j.part0, j.part1]);
        }
        let motors = asm
            .joints()
            .filter(|j| matches!(j.kind, JointKind::Motor | JointKind::Motor6D))
            .copied()
            .collect();
        // Snap the members to where the joints hold them.
        {
            profiling::scope!("Assembly::place");
            let _ = asm.place(dom, root_cf);
        }
        self.bodies.insert(
            handle,
            Body {
                asm,
                colliders,
                motors,
            },
        );
        handle
    }

    fn add_collider(
        &mut self,
        dom: &Dom,
        props: &Props,
        body: RigidBodyHandle,
        part: InstanceId,
        offset: CFrame,
        is_root: bool,
    ) -> Option<(ColliderHandle, Pose)> {
        let Some(Value::Vector3(size)) = dom.property(part, props.size) else {
            return None;
        };
        let (shape, shape_pose) = shape::collider_shape(part_shape(dom, part), size.0)?;
        let can_collide = dom.property(part, props.can_collide) != Some(Value::Bool(false));
        self.read_can_touch(dom, props, part);
        let (mass, material) = collider_surface(dom, part, is_root);
        let (solver, collision) = self.interaction_groups(dom, part);
        let collider = ColliderBuilder::new(shape)
            .mass(mass)
            .friction(material.friction)
            .restitution(material.elasticity)
            .user_data(contacts::pack(&material))
            .active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .solver_groups(solver)
            .collision_groups(collision)
            // Non-colliding parts still fall; they just don't push anything.
            .sensor(!can_collide)
            .position(pose_of(&offset) * shape_pose);
        let handle = self.world.insert_collider(collider, Some(body));
        self.colliders.insert(handle, part);
        Some((handle, shape_pose))
    }

    fn read_can_touch(&mut self, dom: &Dom, props: &Props, part: InstanceId) {
        if dom.property(part, props.can_touch) == Some(Value::Bool(false)) {
            self.touch.untouchable.insert(part);
        } else {
            self.touch.untouchable.remove(&part);
        }
    }

    /// A part's Material, CustomPhysicalProperties or CanTouch changed:
    /// update its collider in place, since its body and joints are the
    /// same. What rests on it reacts at once (observed in Studio 0.742: a
    /// part at rest on a 30° ramp slid as soon as its friction was set to
    /// 0), so its body and everything touching it wake up.
    fn refresh_surface(&mut self, dom: &Dom, props: &Props, h: RigidBodyHandle, part: InstanceId) {
        let Some(body) = self.bodies.get(&h) else {
            return;
        };
        let Some(i) = body.asm.members.iter().position(|m| m.part == part) else {
            return;
        };
        let Some((collider, _)) = body.colliders[i] else {
            return;
        };
        self.read_can_touch(dom, props, part);
        let (mass, material) = collider_surface(dom, part, i == 0);
        let Some(c) = self.world.colliders.get_mut(collider) else {
            return;
        };
        c.set_mass(mass);
        c.set_friction(material.friction);
        c.set_restitution(material.elasticity);
        c.user_data = contacts::pack(&material);
        // Movers read the mass before the next step would recompute it.
        if let Some(rb) = self.world.bodies.get_mut(h) {
            rb.recompute_mass_properties_from_colliders(&self.world.colliders);
        }
        let touching: Vec<RigidBodyHandle> = self
            .world
            .narrow_phase
            .contact_pairs_with(collider)
            .filter_map(|pair| {
                let other = if pair.collider1 == collider {
                    pair.collider2
                } else {
                    pair.collider1
                };
                self.world.colliders.get(other)?.parent()
            })
            .collect();
        self.world.wake_up(h, true);
        for b in touching {
            self.world.wake_up(b, true);
        }
    }

    /// The solver and collision groups for a part's collider. Groups that
    /// don't collide don't push each other; they still touch unless
    /// Workspace.TouchesUseCollisionGroups is on (creator-docs).
    fn interaction_groups(
        &self,
        dom: &Dom,
        part: InstanceId,
    ) -> (InteractionGroups, InteractionGroups) {
        let id = self.groups.id_of(&groups::part_group(dom, part));
        let mask = self.groups.mask_of(id) as u32;
        let solver = InteractionGroups::new(
            Group::from_bits_truncate(1 << id),
            Group::from_bits_truncate(mask),
            InteractionTestMode::And,
        );
        let collision = if self.touches_use_groups {
            solver
        } else {
            InteractionGroups::all()
        };
        (solver, collision)
    }

    /// Join two bodies with a hinge (Rotate, RotateP, RotateV).
    fn add_hinge(&mut self, dom: &Dom, id: InstanceId) {
        let Some(j) = active_joint(dom, id).filter(|j| j.kind == JointKind::Hinge) else {
            return;
        };
        let (Some(&b0), Some(&b1)) = (self.parts.get(&j.part0), self.parts.get(&j.part1)) else {
            return;
        };
        if b0 == b1 {
            return;
        }
        let off0 = self.bodies[&b0].asm.offset_of(j.part0).unwrap_or_default();
        let off1 = self.bodies[&b1].asm.offset_of(j.part1).unwrap_or_default();
        let (c0, c1) = joints::hinge_frames(dom, &j);
        let joint = GenericJointBuilder::new(JointAxesMask::LOCKED_REVOLUTE_AXES)
            .local_frame1(hinge_pose(off0 * c0))
            .local_frame2(hinge_pose(off1 * c1))
            // Inference: hinged parts touch face to face and still turn
            // freely in Roblox, so they mustn't rub.
            .contacts_enabled(false);
        let handle = self.world.insert_impulse_joint(b0, b1, joint);
        self.hinges.insert(id, handle);
        self.joints.insert(id, [j.part0, j.part1]);
        if let Some(input) = joints::hinge_input(dom, &j) {
            self.drives.insert(
                id,
                Driven {
                    joint: handle,
                    source: DriveSource::Surface(input),
                    time: 0.0,
                    angle: None,
                },
            );
        }
    }

    /// Note an instance that may be a mover or a constraint.
    fn notice(&mut self, dom: &Dom, id: InstanceId) {
        let class = dom.class_name(id);
        if movers::is_mover(class) {
            self.mover_ids.insert(id);
        } else if constraints::is_constraint(class) {
            self.constraint_ids.insert(id);
        } else {
            return;
        }
        self.dirty.insert(id);
    }

    /// A part's body and its pose in it.
    fn frame_of(&self, part: InstanceId) -> Option<(RigidBodyHandle, CFrame)> {
        let h = *self.parts.get(&part)?;
        Some((h, self.bodies.get(&h)?.asm.offset_of(part)?))
    }

    /// Re-read the movers that changed (`all` after a rebuild), and every
    /// fired rocket, whose target may have moved.
    fn refresh_movers(&mut self, dom: &Dom, all: bool, dirty: &HashSet<InstanceId>) {
        profiling::scope!("Physics::refresh_movers");
        let ids: Vec<InstanceId> = if all {
            self.mover_ids.iter().copied().collect()
        } else {
            let changed = dirty.iter().filter(|i| self.mover_ids.contains(i));
            changed.chain(self.rockets.keys()).copied().collect()
        };
        let ws = dom.find_service("Workspace");
        for id in ids {
            if dom.is_destroyed(id) {
                self.mover_ids.remove(&id);
                self.rockets.remove(&id);
                self.movers.remove(&id);
                continue;
            }
            let fired = self.rockets.contains_key(&id);
            let mover = ws
                .filter(|ws| dom.is_ancestor_of(*ws, id))
                .and_then(|_| Mover::read(dom, id, fired, |p| self.frame_of(p)));
            match mover {
                Some(m) => self.movers.insert(id, m),
                None => self.movers.remove(&id),
            };
        }
    }

    /// Build, rebuild or remove the joints of constraints that changed.
    fn refresh_constraints(&mut self, dom: &Dom, all: bool, dirty: &HashSet<InstanceId>) {
        profiling::scope!("Physics::refresh_constraints");
        let ids: Vec<InstanceId> = if all {
            self.constraint_ids.iter().copied().collect()
        } else {
            let changed = dirty.iter().filter(|i| self.constraint_ids.contains(i));
            changed.copied().collect()
        };
        let ws = dom.find_service("Workspace");
        for id in ids {
            if dom.is_destroyed(id) {
                self.constraint_ids.remove(&id);
            }
            let wanted = ws
                .filter(|ws| dom.is_ancestor_of(*ws, id))
                .and_then(|_| Spec::read(dom, id))
                .and_then(|spec| {
                    let (b0, o0) = self.frame_of(spec.parts[0])?;
                    let (b1, o1) = self.frame_of(spec.parts[1])?;
                    (b0 != b1).then_some((spec, [b0, b1], [o0, o1]))
                });
            let current = self.constraints.get(&id).filter(|c| {
                self.world.impulse_joints.get(c.joint).is_some()
                    && wanted.as_ref().is_some_and(|(spec, bodies, offsets)| {
                        c.spec == *spec && c.bodies == *bodies && c.offsets == *offsets
                    })
            });
            let joint = match (current, wanted) {
                (Some(c), _) => Some(c.joint),
                (None, wanted) => {
                    if let Some(old) = self.constraints.remove(&id) {
                        self.world.remove_impulse_joint(old.joint);
                    }
                    wanted.map(|(spec, bodies, offsets)| {
                        let joint = self.world.insert_impulse_joint(
                            bodies[0],
                            bodies[1],
                            spec.joint(offsets),
                        );
                        self.constraints.insert(
                            id,
                            BuiltConstraint {
                                spec,
                                bodies,
                                offsets,
                                joint,
                            },
                        );
                        joint
                    })
                }
            };
            match joint.filter(|_| dom.class_name(id) == "HingeConstraint") {
                Some(joint) => {
                    let source = DriveSource::Actuator(hinge_constraint_drive(dom, id));
                    let d = self.drives.entry(id).or_insert(Driven {
                        joint,
                        source,
                        time: 0.0,
                        angle: None,
                    });
                    d.joint = joint;
                    d.source = source;
                }
                None => {
                    self.drives.remove(&id);
                }
            }
        }
    }

    /// Set each driven joint's motor for this frame of `dt` seconds and
    /// `steps` steps.
    fn drive_joints(&mut self, dt: f64, steps: u32) {
        profiling::scope!("Physics::drive_joints");
        for d in self.drives.values_mut() {
            let Some(j) = self.world.impulse_joints.get(d.joint) else {
                continue;
            };
            let (Some(rb1), Some(rb2)) = (
                self.world.bodies.get(j.body1()),
                self.world.bodies.get(j.body2()),
            ) else {
                continue;
            };
            let angle = revolute_angle(&j.data, rb1.position(), rb2.position());
            let drive = match d.source {
                DriveSource::Actuator(drive) => drive,
                DriveSource::Surface(input) => {
                    d.time = if input.input == joints::SurfaceInput::Sin {
                        d.time + dt
                    } else {
                        0.0
                    };
                    let value = input.value(d.time);
                    if input.stepping {
                        let held = *d.angle.get_or_insert(angle);
                        let max = joints::ROTATE_P_STEP * steps as f32;
                        let next = value.map_or(held, |v| held + (v - held).clamp(-max, max));
                        d.angle = Some(next);
                        Drive::Angle { angle: next }
                    } else {
                        // NoInput holds the hinge against anything.
                        Drive::Speed {
                            speed: joints::ROTATE_V_SPEED * value.unwrap_or(0.0),
                            max_torque: value.map_or(Real::MAX, |v| input.strength * v.abs()),
                        }
                    }
                }
            };
            if let Some(new) = apply_drive(&j.data, drive, angle)
                && let Some(j) = self.world.impulse_joints.get_mut(d.joint, true)
            {
                j.data = new;
            }
        }
    }

    /// Re-read a body's joint offsets, move its colliders and write its
    /// parts' CFrames.
    fn refresh_offsets(&mut self, dom: &mut Dom, h: RigidBodyHandle) {
        let Some(body) = self.bodies.get_mut(&h) else {
            return;
        };
        body.asm.refresh_offsets(dom);
        for (m, c) in body.asm.members.iter().zip(&body.colliders) {
            if let Some((collider, shape_pose)) = c
                && let Some(col) = self.world.colliders.get_mut(*collider)
            {
                col.set_position_wrt_parent(pose_of(&m.offset) * *shape_pose);
            }
        }
        let root = cframe_of(self.world.bodies[h].position());
        let _ = body.asm.place(dom, root);
        self.world.wake_up(h, true);
        self.stale.insert(h);
    }

    /// A script moved `part` (or set its velocity): move its body to match.
    fn teleport(&mut self, dom: &mut Dom, props: &Props, h: RigidBodyHandle, part: InstanceId) {
        let Some(body) = self.bodies.get(&h) else {
            return;
        };
        let Some(Value::CFrame(cf)) = dom.property(part, props.cframe) else {
            return;
        };
        let offset = body.asm.offset_of(part).unwrap_or_default();
        let root = cf * offset.inverse();
        let vector = |p| match dom.property(part, p) {
            Some(Value::Vector3(v)) => to_rapier(v.0),
            _ => Vector::ZERO,
        };
        self.stale.insert(h);
        let rb = &mut self.world.bodies[h];
        rb.set_position(pose_of(&root), true);
        if rb.is_dynamic() {
            rb.set_linvel(vector(props.linear_velocity), true);
            rb.set_angvel(vector(props.angular_velocity), true);
        } else {
            rb.user_data = contacts::pack_velocity(vector(props.linear_velocity));
        }
        // Keep the rest of the assembly with it.
        let _ = body.asm.place(dom, root);
    }

    /// Advance the simulation by `dt` seconds in fixed 240 Hz steps.
    pub fn step(&mut self, dt: f64) {
        profiling::scope!("Physics::step");
        self.last_dt = dt;
        if self.query_only || self.bodies.values().all(|b| b.asm.anchored) {
            return;
        }
        let steps = (dt * STEP_HZ).round().max(1.0) as u32;
        self.drive_joints(dt, steps);
        let events = contacts::Collector::default();
        let h = self.world.integration_parameters.dt;
        for _ in 0..steps {
            if !self.movers.is_empty() || !self.forced.is_empty() {
                movers::apply(&mut self.world, self.movers.values(), h, &mut self.forced);
            }
            self.world.step_with_events(&contacts::Mixer, &events);
        }
        // Stepping put every collider in place.
        self.stale.clear();
        for m in self.movers.values() {
            if let Some(inside) = movers::rocket_arrived(&self.world, m)
                && let Some(was) = self.rockets.get_mut(&m.id)
            {
                if inside && !*was {
                    self.reached.push(m.id);
                }
                *was = inside;
            }
        }
        let parts = &self.parts;
        self.touch
            .record(events.take(), &self.colliders, |p| parts.contains_key(&p));
    }

    /// Turn motors toward their DesiredAngle, then write moving parts'
    /// CFrame and velocities into the DOM. Returns the parts that fell
    /// below Workspace.FallenPartsDestroyHeight (the caller destroys them,
    /// as Roblox does).
    ///
    /// The writes don't fire Changed: Roblox doesn't fire property signals
    /// for physics-driven CFrame and velocity changes (Inference from the
    /// creator-docs note on GetPropertyChangedSignal and physics).
    pub fn write_back(&mut self, dom: &mut Dom) -> Vec<InstanceId> {
        profiling::scope!("Physics::write_back");
        let Some(props) = self.props.take() else {
            return Vec::new();
        };
        self.step_motors(dom);
        self.write_hinge_angles(dom);
        let mut fallen = Vec::new();
        for handle in self.world.islands.active_bodies() {
            let Some(body) = self.bodies.get(&handle) else {
                continue;
            };
            let rb = &self.world.bodies[handle];
            let root = cframe_of(rb.position());
            let linvel = Value::Vector3(Vector3(to_glam(rb.linvel())));
            let angvel = Value::Vector3(Vector3(to_glam(rb.angvel())));
            for m in &body.asm.members {
                let cf = root * m.offset;
                if cf.position.y < self.fallen_height {
                    fallen.push(m.part);
                }
                let _ = dom.set_property(m.part, props.cframe, Value::CFrame(cf));
                let _ = dom.set_property(m.part, props.linear_velocity, linvel.clone());
                let _ = dom.set_property(m.part, props.angular_velocity, angvel.clone());
            }
        }
        // Our own writes aren't changes to apply next time.
        self.cursor = dom.journal().end();
        self.props = Some(props);
        fallen
    }

    /// HingeConstraint.CurrentAngle, in degrees.
    fn write_hinge_angles(&self, dom: &mut Dom) {
        for (&id, c) in &self.constraints {
            if !matches!(c.spec.kind, constraints::Kind::Hinge { .. }) {
                continue;
            }
            let (Some(j), Some(rb1), Some(rb2)) = (
                self.world.impulse_joints.get(c.joint),
                self.world.bodies.get(c.bodies[0]),
                self.world.bodies.get(c.bodies[1]),
            ) else {
                continue;
            };
            let angle = revolute_angle(&j.data, rb1.position(), rb2.position());
            let _ =
                dom.set_property_by_name(id, "CurrentAngle", Value::Float32(angle.to_degrees()));
        }
    }

    fn step_motors(&mut self, dom: &mut Dom) {
        let dt = self.last_dt;
        let turned: Vec<RigidBodyHandle> = self
            .bodies
            .iter()
            .filter(|(_, b)| {
                // Every motor steps, even after one has turned.
                b.motors
                    .iter()
                    .fold(false, |any, j| joints::step_motor(dom, j, dt) | any)
            })
            .map(|(h, _)| *h)
            .collect();
        for h in turned {
            self.refresh_offsets(dom, h);
        }
    }
}

#[cfg(test)]
mod tests;
