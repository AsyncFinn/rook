use super::*;
use rapier3d::geometry::NEW_CONTACT_BIT;

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

/// The surface velocity [`pack_velocity`] stored.
pub(crate) fn unpack_velocity(data: u128) -> Vector {
    let [x, y, z, _] = unpack(data);
    Vector::new(x, y, z)
}

fn surface_velocity(context: &ContactModificationContext, body: Option<RigidBodyHandle>) -> Vector {
    body.and_then(|b| context.bodies.get(b))
        .map_or(Vector::ZERO, |rb| unpack_velocity(rb.user_data))
}

/// Merge the points of a contact that has only just begun into one at their
/// middle, for its first step. Rapier bounces each point in turn, once, so
/// the first ones turn the body and the last ones can't undo it: a box
/// landing flat from 2 studs ended on its side, where Studio's stays within
/// 2 degrees (docs/behavior/physics/landing.md).
fn land_as_one(contacts: &mut Vec<SolverContact>) {
    let count = contacts.len();
    if count < 2
        || contacts
            .iter()
            .any(|c| c.contact_id[0] & NEW_CONTACT_BIT == 0)
    {
        return;
    }
    let mut one = contacts[0];
    let share = 1.0 / count as f32;
    one.anchor1 = contacts.iter().map(|c| c.anchor1).sum::<Vector>() * share;
    one.anchor2 = contacts.iter().map(|c| c.anchor2).sum::<Vector>() * share;
    one.dist = contacts.iter().map(|c| c.dist).fold(f32::MAX, f32::min);
    contacts.clear();
    contacts.push(one);
}

/// How deep a falling body's contact may sink before it pushes back, in
/// studs (docs/behavior/physics/landing.md#grazing-an-edge). In Studio a
/// ball falling past a brick that overlaps its edge by a few thousandths
/// drops straight past it, and by 0.06 or more catches it and spins away.
/// Rapier stops any overlap, and within a step it extends a contact along
/// a straight line, so a ball passing an edge looks deeper in than it ever
/// gets: 0.05 lets every graze Studio ignores through.
const SKIN: f32 = 0.05;

/// Sets each contact's friction and elasticity to the two parts' values
/// averaged by their weights (measured in Studio), makes anchored parts
/// with a velocity carry what touches them along, and lets a falling
/// body's contacts sink [`SKIN`] deep before they push back. A body that
/// has just landed keeps the hook one step more without the skin
/// (`Physics::landed`), so the contacts Rapier goes on reusing are plain.
pub(crate) struct Mixer<'a> {
    /// [`crate::Physics::flying`]: the bodies only gravity moved.
    pub(crate) flying: &'a FxHashSet<RigidBodyHandle>,
}

impl Mixer<'_> {
    fn is_flying(&self, body: Option<RigidBodyHandle>) -> bool {
        body.is_some_and(|b| self.flying.contains(&b))
    }
}

impl PhysicsHooks for Mixer<'_> {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        let falling = self.is_flying(context.rigid_body1) || self.is_flying(context.rigid_body2);
        let [f1, fw1, e1, ew1] = unpack(context.colliders[context.collider1].user_data);
        let [f2, fw2, e2, ew2] = unpack(context.colliders[context.collider2].user_data);
        let belt = surface_velocity(context, context.rigid_body1)
            - surface_velocity(context, context.rigid_body2);
        if let Some(m) = context.rigid_mut() {
            *m.friction = mix(f1, fw1, f2, fw2);
            *m.restitution = mix(e1, ew1, e2, ew2);
            if *m.restitution > 0.0 {
                land_as_one(m.solver_contacts);
            }
            if falling {
                for c in m.solver_contacts.iter_mut() {
                    c.dist += SKIN;
                }
            }
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
