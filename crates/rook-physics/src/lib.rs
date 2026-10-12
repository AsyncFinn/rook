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
//! Still to come: BodyMovers and constraints, and queries; see
//! `docs/architecture.md` §12.

pub mod assembly;
mod contacts;
mod convert;
pub mod groups;
pub mod joints;
pub mod materials;
mod shape;
pub mod surfaces;

use std::collections::{HashMap, HashSet};

use rapier3d::prelude::*;
use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::PropertyId;
use rook_types::{CFrame, Value, Vector3};

pub use rook_types::PartShape;
pub use shape::{is_part, part_shape, workspace_parts};

pub use contacts::Touch;

use assembly::{Assembly, is_massless};
use convert::{cframe_of, pose_of, to_glam, to_rapier};
use groups::CollisionGroups;
use joints::{Joint, JointKind, active_joint, endpoints, is_joint};

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
}

impl Props {
    fn resolve(dom: &Dom) -> Option<Self> {
        let db = dom.database();
        let base = db.class_by_name("BasePart")?.id;
        let part = db.class_by_name("Part")?.id;
        let ws = db.class_by_name("Workspace")?.id;
        let p = |class, name| db.find_canonical_property(class, name).map(|p| p.id);
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
        })
    }

    /// Changes that alter which bodies and colliders exist.
    fn is_structural(&self, p: PropertyId) -> bool {
        p == self.size
            || p == self.anchored
            || p == self.can_collide
            || p == self.massless
            || p == self.root_priority
            || p == self.material
            || p == self.custom_properties
            || p == self.collision_group
            || p == self.can_touch
            || Some(p) == self.shape
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
        }
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
        // Our own snapping writes aren't changes to apply next time.
        self.cursor = dom.journal().end();
        self.props = Some(props);
    }

    fn build(&mut self, dom: &mut Dom, props: &Props) {
        profiling::scope!("Physics::build");
        self.read_workspace(dom, props);
        let parts = workspace_parts(dom);
        self.rebuild(dom, props, parts);
        self.built = true;
    }

    fn apply_changes(&mut self, dom: &mut Dom, props: &Props) {
        let mut structural: Vec<InstanceId> = Vec::new();
        let mut changed_joints: Vec<InstanceId> = Vec::new();
        let mut moved_joints: Vec<InstanceId> = Vec::new();
        let mut posed: Vec<InstanceId> = Vec::new();
        let mut workspace_changed = false;
        let db = dom.database();
        for change in dom.journal().since(self.cursor) {
            match change {
                Change::Property { id, property } => {
                    let p = *property;
                    if props.is_structural(p) {
                        structural.push(*id);
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
                    }
                }
                Change::Reparented { subtree, .. } => {
                    for &i in subtree {
                        if self.parts.contains_key(&i) || is_part(dom, i) {
                            structural.push(i);
                        } else if self.joints.contains_key(&i) || is_joint(dom, i) {
                            changed_joints.push(i);
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
        let rebuilt = if structural.is_empty() {
            HashSet::new()
        } else {
            self.rebuild(dom, props, structural)
        };

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
        profiling::scope!("Physics::rebuild");
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
        for part in todo {
            if self.parts.contains_key(&part) {
                continue;
            }
            if let Some(asm) = Assembly::of(dom, part) {
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
        }
        Some(body)
    }

    fn add(&mut self, dom: &mut Dom, props: &Props, asm: Assembly) -> RigidBodyHandle {
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
            RigidBodyBuilder::fixed()
        } else {
            RigidBodyBuilder::dynamic()
                .linvel(vector(props.linear_velocity))
                .angvel(vector(props.angular_velocity))
        };
        let handle = self.world.insert_body(builder.pose(pose_of(&root_cf)));
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
        let _ = asm.place(dom, root_cf);
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
        if dom.property(part, props.can_touch) == Some(Value::Bool(false)) {
            self.touch.untouchable.insert(part);
        }
        // Massless parts add no mass unless they're the root (creator-docs
        // BasePart.Massless).
        let mass = if !is_root && is_massless(dom, part) {
            0.0
        } else {
            part_mass(dom, part)
        };
        let material = materials::of_part(dom, part);
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
        let rb = &mut self.world.bodies[h];
        rb.set_position(pose_of(&root), true);
        if rb.is_dynamic() {
            rb.set_linvel(vector(props.linear_velocity), true);
            rb.set_angvel(vector(props.angular_velocity), true);
        }
        // Keep the rest of the assembly with it.
        let _ = body.asm.place(dom, root);
    }

    /// Advance the simulation by `dt` seconds in fixed 240 Hz steps.
    pub fn step(&mut self, dt: f64) {
        profiling::scope!("Physics::step");
        self.last_dt = dt;
        if self.bodies.values().all(|b| b.asm.anchored) {
            return;
        }
        let steps = (dt * STEP_HZ).round().max(1.0) as u32;
        let events = contacts::Collector::default();
        for _ in 0..steps {
            self.world.step_with_events(&contacts::Mixer, &events);
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
