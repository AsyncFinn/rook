//! Roblox physics semantics over Rapier.
//!
//! [`Physics`] mirrors Workspace's parts into a Rapier world. It reads the
//! DOM's change journal to keep up with scripts, steps the simulation, and
//! writes simulated parts' CFrame and velocities back into the DOM.
//!
//! What exists so far (M3 step 1): anchored parts are fixed colliders and
//! every unanchored part is its own rigid body. Joints, assemblies, contact
//! events and BodyMovers come next; see `docs/architecture.md` §8.

mod convert;
mod shape;

use std::collections::{HashMap, HashSet};

use rapier3d::prelude::*;
use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::PropertyId;
use rook_types::{CFrame, Value, Vector3};

pub use shape::{PartShape, part_shape};

use convert::{cframe_of, pose_of, to_glam, to_rapier};

/// Roblox simulates at 240 Hz (creator-docs "Adaptive timestepping": 240,
/// 120 or 60 Hz per assembly; Rook always uses 240, an Approximation).
pub const STEP_HZ: f64 = 240.0;

/// Workspace.Gravity's default, in studs/s² (creator-docs Workspace.Gravity).
const DEFAULT_GRAVITY: f32 = 196.2;

/// Workspace.FallenPartsDestroyHeight's default (creator-docs).
const DEFAULT_FALLEN_HEIGHT: f32 = -500.0;

/// Plastic, the default material (creator-docs "Physical properties":
/// density 0.7, friction 0.3, elasticity 0.5). Every part uses it until
/// materials and CustomPhysicalProperties are read (an Approximation).
const PLASTIC_DENSITY: f32 = 0.7;
const PLASTIC_FRICTION: f32 = 0.3;
const PLASTIC_ELASTICITY: f32 = 0.5;

/// The physics properties [`Physics`] watches, resolved once.
#[derive(Debug)]
struct Props {
    cframe: PropertyId,
    size: PropertyId,
    anchored: PropertyId,
    can_collide: PropertyId,
    shape: Option<PropertyId>,
    linear_velocity: PropertyId,
    angular_velocity: PropertyId,
    gravity: PropertyId,
    fallen_height: PropertyId,
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
            shape: p(part, "Shape"),
            linear_velocity: p(base, "AssemblyLinearVelocity")?,
            angular_velocity: p(base, "AssemblyAngularVelocity")?,
            gravity: p(ws, "Gravity")?,
            fallen_height: p(ws, "FallenPartsDestroyHeight")?,
        })
    }

    fn is_physical(&self, p: PropertyId) -> bool {
        p == self.cframe
            || p == self.size
            || p == self.anchored
            || p == self.can_collide
            || Some(p) == self.shape
            || p == self.linear_velocity
            || p == self.angular_velocity
    }
}

/// One part in the Rapier world.
#[derive(Debug, Clone, Copy)]
struct PartBody {
    collider: ColliderHandle,
    /// `None` for anchored parts: their colliders sit in the world directly.
    body: Option<RigidBodyHandle>,
}

/// A DataModel's physics world.
pub struct Physics {
    world: PhysicsWorld,
    props: Option<Props>,
    parts: HashMap<InstanceId, PartBody>,
    bodies: HashMap<RigidBodyHandle, InstanceId>,
    /// Journal position up to which changes have been applied.
    cursor: u64,
    built: bool,
    fallen_height: f32,
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
        Self {
            world,
            props: None,
            parts: HashMap::new(),
            bodies: HashMap::new(),
            cursor: 0,
            built: false,
            fallen_height: DEFAULT_FALLEN_HEIGHT,
        }
    }

    /// Parts in the world.
    pub fn part_count(&self) -> usize {
        self.parts.len()
    }

    /// Simulated (unanchored) parts.
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Bring the world up to date with the DOM: on the first call, add
    /// every Workspace part; after that, apply the journal since last time.
    pub fn sync(&mut self, dom: &Dom) {
        profiling::scope!("Physics::sync");
        if self.props.is_none() {
            self.props = Props::resolve(dom);
        }
        let Some(props) = self.props.take() else {
            return;
        };
        let end = dom.journal().end();
        if !self.built {
            self.build(dom, &props);
        } else {
            let mut dirty: HashSet<InstanceId> = HashSet::new();
            let mut workspace_changed = false;
            for change in dom.journal().since(self.cursor) {
                match change {
                    Change::Property { id, property } => {
                        if props.is_physical(*property) {
                            dirty.insert(*id);
                        } else if *property == props.gravity || *property == props.fallen_height {
                            workspace_changed = true;
                        }
                    }
                    Change::Reparented { subtree, .. } => dirty.extend(subtree.iter().copied()),
                    _ => {}
                }
            }
            if workspace_changed {
                self.read_workspace(dom, &props);
            }
            let workspace = dom.find_service("Workspace");
            for id in dirty {
                self.refresh(dom, &props, workspace, id);
            }
        }
        self.cursor = end;
        self.props = Some(props);
    }

    fn build(&mut self, dom: &Dom, props: &Props) {
        profiling::scope!("Physics::build");
        self.read_workspace(dom, props);
        if let Some(ws) = dom.find_service("Workspace") {
            for id in dom.descendants(ws) {
                if is_simulated_part(dom, id) {
                    self.add(dom, props, id);
                }
            }
        }
        self.built = true;
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
    }

    /// Re-read one instance: drop what the world has for it, then add it
    /// again if it's a part in Workspace.
    fn refresh(&mut self, dom: &Dom, props: &Props, ws: Option<InstanceId>, id: InstanceId) {
        self.remove(id);
        let in_workspace = ws.is_some_and(|ws| dom.is_ancestor_of(ws, id));
        if in_workspace && is_simulated_part(dom, id) {
            self.add(dom, props, id);
        }
    }

    fn remove(&mut self, id: InstanceId) {
        let Some(part) = self.parts.remove(&id) else {
            return;
        };
        match part.body {
            Some(body) => {
                self.bodies.remove(&body);
                self.world.remove_body(body);
            }
            None => {
                self.world.remove_collider(part.collider);
            }
        }
    }

    fn add(&mut self, dom: &Dom, props: &Props, id: InstanceId) {
        let Some(Value::CFrame(cf)) = dom.property(id, props.cframe) else {
            return;
        };
        let Some(Value::Vector3(size)) = dom.property(id, props.size) else {
            return;
        };
        let shape = part_shape(dom, id);
        let Some((shape, offset)) = shape::collider_shape(shape, size.0) else {
            return;
        };
        let can_collide = dom.property(id, props.can_collide) != Some(Value::Bool(false));
        let anchored = dom.property(id, props.anchored) == Some(Value::Bool(true));
        let pose = pose_of(&cf);
        let collider = ColliderBuilder::new(shape)
            .density(PLASTIC_DENSITY)
            .friction(PLASTIC_FRICTION)
            .restitution(PLASTIC_ELASTICITY)
            // Non-colliding parts still fall; they just don't push anything.
            .sensor(!can_collide);
        if anchored {
            let handle = self
                .world
                .insert_collider(collider.position(pose * offset), None);
            self.parts.insert(
                id,
                PartBody {
                    collider: handle,
                    body: None,
                },
            );
            return;
        }
        let vector = |p| match dom.property(id, p) {
            Some(Value::Vector3(v)) => to_rapier(v.0),
            _ => Vector::ZERO,
        };
        let body = RigidBodyBuilder::dynamic()
            .pose(pose)
            .linvel(vector(props.linear_velocity))
            .angvel(vector(props.angular_velocity));
        let (body, collider) = self.world.insert(body, collider.position(offset));
        self.bodies.insert(body, id);
        self.parts.insert(
            id,
            PartBody {
                collider,
                body: Some(body),
            },
        );
    }

    /// Advance the simulation by `dt` seconds in fixed 240 Hz steps.
    pub fn step(&mut self, dt: f64) {
        profiling::scope!("Physics::step");
        if self.bodies.is_empty() {
            return;
        }
        let steps = (dt * STEP_HZ).round().max(1.0) as u32;
        for _ in 0..steps {
            self.world.step();
        }
    }

    /// Write moving parts' CFrame and velocities into the DOM, and return
    /// the parts that fell below Workspace.FallenPartsDestroyHeight (the
    /// caller destroys them, as Roblox does).
    ///
    /// The writes don't fire Changed: Roblox doesn't fire property signals
    /// for physics-driven CFrame and velocity changes (Inference from the
    /// creator-docs note on GetPropertyChangedSignal and physics).
    pub fn write_back(&mut self, dom: &mut Dom) -> Vec<InstanceId> {
        profiling::scope!("Physics::write_back");
        let Some(props) = &self.props else {
            return Vec::new();
        };
        let mut fallen = Vec::new();
        for handle in self.world.islands.active_bodies() {
            let Some(&id) = self.bodies.get(&handle) else {
                continue;
            };
            let body = &self.world.bodies[handle];
            let cf: CFrame = cframe_of(body.position());
            if cf.position.y < self.fallen_height {
                fallen.push(id);
            }
            let _ = dom.set_property(id, props.cframe, Value::CFrame(cf));
            let _ = dom.set_property(
                id,
                props.linear_velocity,
                Value::Vector3(Vector3(to_glam(body.linvel()))),
            );
            let _ = dom.set_property(
                id,
                props.angular_velocity,
                Value::Vector3(Vector3(to_glam(body.angvel()))),
            );
        }
        // Our own writes aren't changes to apply next time.
        self.cursor = dom.journal().end();
        fallen
    }
}

fn is_simulated_part(dom: &Dom, id: InstanceId) -> bool {
    dom.is_a(id, "BasePart") && dom.class_name(id) != "Terrain"
}

#[cfg(test)]
mod tests;
