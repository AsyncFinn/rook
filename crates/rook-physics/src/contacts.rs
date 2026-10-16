//! What happens where parts meet: friction and elasticity mixed the way
//! Roblox mixes them, and Rapier's collision events turned into Touched
//! and TouchEnded.
//!
//! Touch behavior observed in Studio 0.742: both parts fire, each with
//! the other as the argument; bouncing fires Touched and TouchEnded again;
//! non-colliding parts touch too; nothing fires if either part has CanTouch
//! off; anchored parts don't touch each other; destroying a part fires
//! nothing. Rook follows Rapier's contacts, which start slightly before
//! parts meet and end slightly after, where Roblox ends a touch as soon as
//! parts stop overlapping (Approximation).

pub mod groups;
pub mod materials;

use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::sync::Mutex;

use rapier3d::prelude::*;
use rook_dom::InstanceId;

use crate::contacts::materials::{Properties, mix};

/// A Touched (`began`) or TouchEnded event: fire it on `part` with `other`
/// as the argument, then on `other` with `part`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Touch {
    pub part: InstanceId,
    pub other: InstanceId,
    pub began: bool,
}

/// A collider's friction, elasticity and their weights, packed into its
/// `user_data` for [`Mixer`].
pub(crate) fn pack(p: &Properties) -> u128 {
    let bits = [
        p.friction,
        p.friction_weight,
        p.elasticity,
        p.elasticity_weight,
    ];
    bits.iter()
        .enumerate()
        .fold(0, |acc, (i, f)| acc | (u128::from(f.to_bits()) << (32 * i)))
}

fn unpack(data: u128) -> [f32; 4] {
    std::array::from_fn(|i| f32::from_bits((data >> (32 * i)) as u32))
}

/// An anchored body's `user_data`: its surface velocity, for conveyors.
pub(crate) fn pack_velocity(v: Vector) -> u128 {
    [v.x, v.y, v.z]
        .iter()
        .enumerate()
        .fold(0, |acc, (i, f)| acc | (u128::from(f.to_bits()) << (32 * i)))
}

/// Whether a collider needs [`Mixer`]: its weights aren't Rapier's plain
/// average (both 1), or its anchored body carries what touches it. Pairs
/// without it get the same friction and elasticity from Rapier's Average
/// rule, and Rapier can reuse their contacts while they barely move, which
/// it never does for a pair with a hook (half the narrow phase on 60k-part
/// places).
pub(crate) fn hooks_for(collider_data: u128, body_data: u128) -> ActiveHooks {
    let [_, fw, _, ew] = unpack(collider_data);
    if fw != 1.0 || ew != 1.0 || body_data != 0 {
        ActiveHooks::MODIFY_SOLVER_CONTACTS
    } else {
        ActiveHooks::empty()
    }
}

fn surface_velocity(context: &ContactModificationContext, body: Option<RigidBodyHandle>) -> Vector {
    let Some(rb) = body.and_then(|b| context.bodies.get(b)) else {
        return Vector::ZERO;
    };
    let [x, y, z, _] = unpack(rb.user_data);
    Vector::new(x, y, z)
}

/// Sets each contact's friction and elasticity to the two parts' values
/// averaged by their weights (measured in Studio), and makes anchored parts
/// with a velocity carry what touches them along.
pub(crate) struct Mixer;

impl PhysicsHooks for Mixer {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        let [f1, fw1, e1, ew1] = unpack(context.colliders[context.collider1].user_data);
        let [f2, fw2, e2, ew2] = unpack(context.colliders[context.collider2].user_data);
        let belt = surface_velocity(context, context.rigid_body1)
            - surface_velocity(context, context.rigid_body2);
        if let Some(m) = context.rigid_mut() {
            *m.friction = mix(f1, fw1, f2, fw2);
            *m.restitution = mix(e1, ew1, e2, ew2);
            if belt != Vector::ZERO {
                let n = *m.normal;
                let tangent = belt - n * belt.dot(n);
                for c in m.solver_contacts.iter_mut() {
                    c.tangent_velocity = tangent;
                }
            }
        }
    }
}

/// Collects Rapier's collision events during a step.
#[derive(Default)]
pub(crate) struct Collector(Mutex<Vec<CollisionEvent>>);

impl Collector {
    pub(crate) fn take(&self) -> Vec<CollisionEvent> {
        std::mem::take(&mut self.0.lock().expect("not poisoned"))
    }
}

impl EventHandler for Collector {
    fn handle_collision_event(
        &self,
        _: &RigidBodySet,
        _: &ColliderSet,
        event: CollisionEvent,
        _: Option<&ContactPair>,
    ) {
        self.0.lock().expect("not poisoned").push(event);
    }

    fn handle_contact_force_event(
        &self,
        _: Real,
        _: &RigidBodySet,
        _: &ColliderSet,
        _: &ContactPair,
        _: Real,
    ) {
    }

    fn handle_soft_body_tear_event(&self, _: &SoftBodySet, _: &SoftBodyTearEvent) {}
}

/// Which parts touch, and the Touched/TouchEnded events not yet fired.
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    /// Touching pairs, lower id first.
    touching: HashSet<(InstanceId, InstanceId)>,
    /// Colliders removed since the last step: Rapier reports their
    /// contacts ending after they're gone.
    retired: HashMap<ColliderHandle, InstanceId>,
    /// Parts with CanTouch off.
    pub(crate) untouchable: HashSet<InstanceId>,
    events: Vec<Touch>,
}

impl Tracker {
    pub(crate) fn retire(&mut self, collider: ColliderHandle, part: InstanceId) {
        self.retired.insert(collider, part);
    }

    pub(crate) fn take(&mut self) -> Vec<Touch> {
        std::mem::take(&mut self.events)
    }

    fn push(&mut self, (part, other): (InstanceId, InstanceId), began: bool) {
        if !self.untouchable.contains(&part) && !self.untouchable.contains(&other) {
            self.events.push(Touch { part, other, began });
        }
    }

    /// Turn one frame's collision events into touches. A collider removed
    /// and re-added in the same frame (its body was rebuilt) keeps touching
    /// without firing anything; one removed for good ends its touches
    /// silently if its part left the world, or with TouchEnded if not.
    pub(crate) fn record(
        &mut self,
        events: Vec<CollisionEvent>,
        colliders: &HashMap<ColliderHandle, InstanceId>,
        in_world: impl Fn(InstanceId) -> bool,
    ) {
        let mut removed: Vec<(InstanceId, InstanceId)> = Vec::new();
        for event in events {
            let part = |h| colliders.get(&h).or_else(|| self.retired.get(&h)).copied();
            let (Some(a), Some(b)) = (part(event.collider1()), part(event.collider2())) else {
                continue;
            };
            if a == b {
                continue;
            }
            let pair = (a.min(b), a.max(b));
            match event {
                CollisionEvent::Started(..) => {
                    if let Some(pos) = removed.iter().position(|p| *p == pair) {
                        removed.swap_remove(pos);
                    } else if self.touching.insert(pair) {
                        self.push(pair, true);
                    }
                }
                CollisionEvent::Stopped(_, _, flags)
                    if flags.contains(CollisionEventFlags::REMOVED) =>
                {
                    if self.touching.contains(&pair) && !removed.contains(&pair) {
                        removed.push(pair);
                    }
                }
                CollisionEvent::Stopped(..) => {
                    if self.touching.remove(&pair) {
                        self.push(pair, false);
                    }
                }
            }
        }
        for pair in removed {
            self.touching.remove(&pair);
            if in_world(pair.0) && in_world(pair.1) {
                self.push(pair, false);
            }
        }
        self.retired.clear();
    }

    /// Forget a part that left the world.
    pub(crate) fn forget(&mut self, part: InstanceId) {
        self.untouchable.remove(&part);
    }
}
