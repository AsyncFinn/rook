//! Explosion's blast (docs/behavior/effects/explosions.md), measured in Studio
//! 0.742: which parts it hits, the joints it breaks, how hard it pushes
//! and the crater it leaves in Terrain.
//!
//! The engine carries a blast out in two halves: [`Physics::blast`] finds
//! what the explosion does, the engine destroys the joints and fires
//! `Hit`, and [`Physics::push`] then pushes the parts, now free of their
//! broken joints.

use std::cell::RefCell;

use glam::Vec3;
use rapier3d::prelude::*;
use rook_dom::{Dom, InstanceId};
use rook_terrain::TerrainMaterial;
use rook_terrain::grid::edit::Shape as Fill;
use rustc_hash::FxHashMap as HashMap;

use crate::convert::{to_glam, to_rapier};
use crate::joints::{JointKind, part_joints};
use crate::query::Filter;
use crate::{Physics, shape};

/// A part's push per unit of `BlastPressure` and square stud of its size:
/// the impulse is `BlastPressure × |Size|² × PUSH`, whatever its distance,
/// mass or shape (measured: 0.82236 for a 1-stud cube at pressure 5,000;
/// 27.96 for 1 × 1 × 10; 7.9496 for 2 × 3 × 4 in any orientation).
const PUSH: f32 = 5.4825e-5;

/// What an explosion does.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Blast {
    /// Each part it hits, by id, and its center's distance from the
    /// explosion, for `Hit`.
    pub hits: Vec<(InstanceId, f32)>,
    /// Joints it breaks.
    pub joints: Vec<InstanceId>,
    /// Parts it pushes: the impulse, and the spin about its direction.
    pushes: Vec<(InstanceId, Vec3, f32)>,
}

impl Physics {
    /// Explosion `explosion` goes off (it has just entered Workspace): it hits
    /// the parts its radius reaches, breaks their joints, pushes them
    /// ([`PUSH`]) and carves Terrain, by the rules measured in Studio
    /// (`docs/behavior/effects/explosions.md`).
    pub fn blast(&mut self, dom: &mut Dom, explosion: InstanceId) -> Blast {
        profiling::scope!("Physics::blast");
        let number = |name, default: f64| dom.number_property(explosion, name).unwrap_or(default);
        let center = dom
            .vector3_property(explosion, "Position")
            .unwrap_or(Vec3::ZERO);
        let radius = number("BlastRadius", 4.0).max(0.0) as f32;
        let pressure = number("BlastPressure", 500_000.0) as f32;
        let breaks = radius * number("DestroyJointRadiusPercent", 1.0).clamp(0.0, 1.0) as f32;
        // Craters unless set otherwise, as for a new Explosion.
        let craters = dom.enum_item_property(explosion, "ExplosionType") != Some("NoCraters");
        self.prepare(dom);
        let mut blast = Blast::default();
        if radius > 0.0 {
            let ball = Ball::new(radius);
            let pose = Pose::from_translation(to_rapier(center));
            let point = to_rapier(center);
            let reach = |co: &Collider| {
                let surface = co.shape().project_point(co.position(), point, false);
                (to_glam(surface.point) - center).length()
            };
            // Each part's nearest collider surface.
            let reached = RefCell::new(HashMap::default());
            let near = self.overlapping(
                dom,
                (&ball, pose),
                &Filter::default(),
                0,
                None,
                |part, co| {
                    let d = reach(co);
                    let mut reached = reached.borrow_mut();
                    let least = reached.entry(part).or_insert(f32::MAX);
                    *least = d.min(*least);
                    d <= radius
                },
            );
            let reached = reached.into_inner();
            for part in near {
                let Some((size, at)) = size_and_position(dom, part) else {
                    continue;
                };
                blast.hits.push((part, (at - center).length()));
                if shielded(dom, part) {
                    continue;
                }
                if breaks > 0.0 && reached.get(&part).is_some_and(|&d| d <= breaks) {
                    for j in part_joints(dom, part) {
                        let breakable = matches!(
                            j.kind,
                            JointKind::Fixed | JointKind::Motor | JointKind::Motor6D
                        );
                        if breakable && !blast.joints.contains(&j.id) {
                            blast.joints.push(j.id);
                        }
                    }
                }
                let impulse = pressure * size.length_squared() * PUSH;
                let way = (at - center).try_normalize().unwrap_or(Vec3::Y);
                blast
                    .pushes
                    .push((part, way * impulse, impulse * size.length() / 4.0));
            }
        }
        if craters
            && radius > 0.0
            && let Some(terrain) = self.terrain.instance
        {
            let fill = Fill::Ball { center, radius };
            self.edit_terrain(dom, terrain, |g| ((), g.fill(&fill, TerrainMaterial::Air)));
        }
        blast
    }

    /// Push the parts `blast` hit, after its joints are gone. A world that
    /// only answers queries (a client's) pushes nothing.
    pub fn push(&mut self, dom: &mut Dom, blast: &Blast) {
        if self.query_only || blast.pushes.is_empty() {
            return;
        }
        self.sync(dom);
        for &(part, impulse, spin) in &blast.pushes {
            let Some(&h) = self.parts.get(&part) else {
                continue;
            };
            let Some(at) = size_and_position(dom, part).map(|(_, at)| at) else {
                continue;
            };
            let Some(body) = self.world.bodies.get_mut(h) else {
                continue;
            };
            if !body.is_dynamic() {
                continue;
            }
            let way = impulse.normalize_or_zero();
            body.apply_impulse_at_point(to_rapier(impulse), to_rapier(at), true);
            body.apply_torque_impulse(to_rapier(way * spin), true);
        }
    }
}

/// A part's `Size` and where its center is.
fn size_and_position(dom: &Dom, part: InstanceId) -> Option<(Vec3, Vec3)> {
    let size = dom.vector3_property(part, "Size")?;
    let at = dom.cframe_property(part, "CFrame")?.position;
    Some((size.max(Vec3::splat(shape::MIN_PART_SIZE)), at))
}

/// Whether something `part` is under holds a ForceField.
fn shielded(dom: &Dom, part: InstanceId) -> bool {
    let mut at = dom.parent(part);
    while let Some(holder) = at {
        if dom
            .children(holder)
            .iter()
            .any(|&c| dom.class_name(c) == "ForceField")
        {
            return true;
        }
        at = dom.parent(holder);
    }
    false
}
