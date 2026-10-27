//! Friction and elasticity mixed the way Roblox mixes them, and anchored
//! parts with a velocity carrying what touches them.

use super::*;

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
