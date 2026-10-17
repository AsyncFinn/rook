//! Movers: instances that push an assembly with forces each 240 Hz step.
//! The legacy BodyMovers (BodyForce, BodyThrust, BodyVelocity,
//! BodyAngularVelocity, BodyPosition, BodyGyro, RocketPropulsion) act on
//! the assembly of the part they're parented to; VectorForce, Torque and
//! SpringConstraint act through attachments.
//!
//! Rules observed in Studio 0.742 unless marked otherwise:
//!
//! - A mover acts on its part's whole assembly, with the assembly's mass,
//!   at the centre of mass (no torque) unless it has a point of its own.
//!   Several movers add up. Anchored assemblies don't move.
//! - Limits (MaxForce, MaxTorque) clamp each world axis separately.
//! - BodyVelocity and BodyAngularVelocity reach their target within one
//!   frame whatever their P: they're velocity constraints whose only limit
//!   is MaxForce or MaxTorque, a true force and torque. BodyVelocity holds
//!   against gravity on the axes it may push along.
//! - BodyPosition and BodyGyro are spring-dampers whose response doesn't
//!   depend on mass or inertia ([`damping`] for how D scales). BodyGyro's
//!   gains act at a quarter strength, on the sine of the angle, and its
//!   MaxTorque limits angular *acceleration*: a cube 2 or 4 studs wide
//!   speeds up by the same 1 rad/s² per unit of MaxTorque.
//! - RocketPropulsion, once fired, flies straight at its target at
//!   MaxSpeed, holding against gravity, with at most MaxThrust (ThrustP
//!   made no difference). It turns like a BodyGyro (TurnP, TurnD,
//!   MaxTorque) toward a direction between the target's (CartoonFactor 1)
//!   and its thrust's (0). Within TargetRadius it fires ReachedTarget and
//!   holds still until Abort.
//! - An assembly a mover only holds in place (a BodyGyro or BodyPosition
//!   at its target, a zero BodyVelocity, a force friction balances) falls
//!   asleep like any resting one, hinged vehicles included. Changing the
//!   mover wakes it. A mover asking for motion keeps it awake: BodyVelocity
//!   at 0.03 studs/s and BodyAngularVelocity at 0.05 rad/s did, at 0.01
//!   neither did (Approximation: any nonzero target keeps it awake).

use rustc_hash::FxHashMap as HashMap;

use glam::{Mat3, Vec3};
use rapier3d::prelude::*;
use rook_dom::{Dom, InstanceId};
use rook_types::CFrame;

use crate::changes::read::{boolean, cframe, enum_value, float, vector};
use crate::convert::{cframe_of, to_glam, to_rapier};
use crate::joints::constraints::{attachment, attachments};

/// Classes that act through [`Mover`].
pub(crate) fn is_mover(class: &str) -> bool {
    matches!(
        class,
        "BodyForce"
            | "BodyThrust"
            | "BodyVelocity"
            | "BodyAngularVelocity"
            | "BodyPosition"
            | "BodyGyro"
            | "RocketPropulsion"
            | "VectorForce"
            | "Torque"
            | "SpringConstraint"
    )
}

/// A frame fixed in a body: the body and the frame's pose in it.
pub(crate) type Frame = (RigidBodyHandle, CFrame);

/// A mover acting on a body this frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mover {
    pub id: InstanceId,
    /// The body pushed, and the mover's frame in it: its part's, or
    /// Attachment0's.
    pub at: Frame,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Kind {
    /// A force along world axes or `relative` to a frame, at the centre of
    /// mass or at `point` in the mover's frame.
    Force {
        force: Vec3,
        relative: Option<Frame>,
        point: Option<Vec3>,
    },
    Torque {
        torque: Vec3,
        relative: Option<Frame>,
    },
    Velocity {
        target: Vec3,
        max_force: Vec3,
    },
    AngularVelocity {
        target: Vec3,
        max_torque: Vec3,
    },
    Position {
        target: Vec3,
        p: f32,
        d: f32,
        max_force: Vec3,
    },
    Gyro {
        target: Mat3,
        p: f32,
        d: f32,
        max_accel: Vec3,
    },
    Rocket {
        target: Vec3,
        max_speed: f32,
        max_thrust: f32,
        radius: f32,
        turn_p: f32,
        turn_d: f32,
        max_accel: Vec3,
        cartoon: f32,
    },
    /// Pulls the mover's frame and `other` together along the line between
    /// them.
    Spring {
        other: Frame,
        stiffness: f32,
        damping: f32,
        free_length: f32,
        max_force: f32,
    },
}

/// How BodyPosition's and BodyGyro's D acts. The settled approach speed
/// matches a damper of D up to about 384, then of D²/384 (observed for D
/// from 70 to 5000 with P 1000 and 10000; Approximation of whatever
/// Roblox does).
fn damping(d: f32) -> f32 {
    let d = d.abs();
    d.max(d * d / 384.0)
}

/// RocketPropulsion's target point: the Target part's position plus
/// TargetOffset, or TargetOffset alone without a Target (Inference).
fn rocket_target(dom: &Dom, id: InstanceId) -> Vec3 {
    let offset = vector(dom, id, "TargetOffset");
    match dom.ref_property(id, "Target") {
        Some(t) => cframe(dom, t, "CFrame").position + offset,
        None => offset,
    }
}

impl Mover {
    /// Read mover `id`. `frame_of` gives a part's body and its pose in it.
    /// Rockets act only once `fired`.
    pub(crate) fn read(
        dom: &Dom,
        id: InstanceId,
        fired: bool,
        frame_of: impl Fn(InstanceId) -> Option<Frame>,
    ) -> Option<Self> {
        let class = dom.class_name(id);
        let in_frame = |att: InstanceId| {
            let (part, cf) = attachment(dom, att)?;
            let (body, offset) = frame_of(part)?;
            Some((body, offset * cf))
        };
        // Constraints act through Attachment0 (and Attachment1).
        if matches!(class, "VectorForce" | "Torque" | "SpringConstraint") {
            if dom.is_destroyed(id) || !boolean(dom, id, "Enabled", true) {
                return None;
            }
            let a0 = dom.ref_property(id, "Attachment0")?;
            let at = in_frame(a0)?;
            // ActuatorRelativeTo: Attachment0 0, Attachment1 1, World 2.
            let relative = || match enum_value(dom, id, "RelativeTo") {
                0 => Some(at),
                1 => dom.ref_property(id, "Attachment1").and_then(in_frame),
                _ => None,
            };
            let kind = match class {
                "VectorForce" => Kind::Force {
                    force: vector(dom, id, "Force"),
                    relative: relative(),
                    point: (!boolean(dom, id, "ApplyAtCenterOfMass", false)).then_some(Vec3::ZERO),
                },
                "Torque" => Kind::Torque {
                    torque: vector(dom, id, "Torque"),
                    relative: relative(),
                },
                _ => {
                    let [_, a1] = attachments(dom, id)?;
                    Kind::Spring {
                        other: in_frame(a1)?,
                        stiffness: float(dom, id, "Stiffness"),
                        damping: float(dom, id, "Damping"),
                        free_length: float(dom, id, "FreeLength"),
                        max_force: float(dom, id, "MaxForce"),
                    }
                }
            };
            return Some(Self { id, at, kind });
        }
        let at = frame_of(dom.parent(id)?)?;
        let kind = match class {
            "BodyForce" => Kind::Force {
                force: vector(dom, id, "Force"),
                relative: None,
                point: None,
            },
            // The force is in the part's frame, at Location in it.
            "BodyThrust" => Kind::Force {
                force: vector(dom, id, "Force"),
                relative: Some(at),
                point: Some(vector(dom, id, "Location")),
            },
            "BodyVelocity" => Kind::Velocity {
                target: vector(dom, id, "Velocity"),
                max_force: vector(dom, id, "MaxForce"),
            },
            "BodyAngularVelocity" => Kind::AngularVelocity {
                target: vector(dom, id, "AngularVelocity"),
                max_torque: vector(dom, id, "MaxTorque"),
            },
            "BodyPosition" => Kind::Position {
                target: vector(dom, id, "Position"),
                p: float(dom, id, "P"),
                d: damping(float(dom, id, "D")),
                max_force: vector(dom, id, "MaxForce"),
            },
            "BodyGyro" => Kind::Gyro {
                target: cframe(dom, id, "CFrame").rotation,
                p: float(dom, id, "P"),
                d: damping(float(dom, id, "D")),
                max_accel: vector(dom, id, "MaxTorque"),
            },
            "RocketPropulsion" if fired => Kind::Rocket {
                target: rocket_target(dom, id),
                max_speed: float(dom, id, "MaxSpeed"),
                max_thrust: float(dom, id, "MaxThrust"),
                radius: float(dom, id, "TargetRadius"),
                turn_p: float(dom, id, "TurnP"),
                turn_d: damping(float(dom, id, "TurnD")),
                max_accel: vector(dom, id, "MaxTorque"),
                cartoon: float(dom, id, "CartoonFactor").clamp(0.0, 1.0),
            },
            _ => return None,
        };
        Some(Self { id, at, kind })
    }
}

/// A body's state at the start of a step.
struct State {
    pose: CFrame,
    com: Vec3,
    v: Vec3,
    w: Vec3,
    mass: f32,
    inertia: AngularInertia,
    dynamic: bool,
    awake: bool,
}

impl State {
    fn of(rb: &RigidBody) -> Self {
        Self {
            pose: cframe_of(rb.position()),
            com: to_glam(rb.center_of_mass()),
            v: to_glam(rb.linvel()),
            w: to_glam(rb.angvel()),
            mass: rb.mass(),
            inertia: rb.mass_properties().effective_angular_inertia(),
            dynamic: rb.is_dynamic(),
            awake: rb.is_dynamic() && !rb.is_sleeping(),
        }
    }

    fn velocity_at(&self, p: Vec3) -> Vec3 {
        self.v + self.w.cross(p - self.com)
    }

    /// The torque that gives angular acceleration `alpha`.
    fn torque_for(&self, alpha: Vec3) -> Vec3 {
        to_glam(self.inertia * to_rapier(alpha))
    }
}

fn clamp_axes(v: Vec3, max: Vec3) -> Vec3 {
    let max = max.abs();
    v.clamp(-max, max)
}

/// One implicit step of a spring-damper's acceleration (stable for any
/// gains, which Roblox's defaults need at 240 Hz). `external` is the
/// acceleration acting besides (gravity), solved with it so the spring
/// rests where `p · err` balances it, as in Roblox.
fn spring_accel(err: Vec3, v: Vec3, p: f32, d: f32, h: f32, external: Vec3) -> Vec3 {
    let next = (v + h * (p * err + external)) / (1.0 + h * d + h * h * p);
    (next - v) / h - external
}

/// The axis-times-sine rotation from `r` to `target`.
fn rotation_error(r: Mat3, target: Mat3) -> Vec3 {
    0.5 * (r.x_axis.cross(target.x_axis)
        + r.y_axis.cross(target.y_axis)
        + r.z_axis.cross(target.z_axis))
}

/// A BodyGyro-style turn toward an error, as an angular acceleration.
fn gyro_accel(err: Vec3, w: Vec3, p: f32, d: f32, h: f32) -> Vec3 {
    spring_accel(err, w, p / 4.0, d / 4.0, h, Vec3::ZERO)
}

/// What a mover adds to a body this step.
#[derive(Default)]
struct Push {
    force: Vec3,
    torque: Vec3,
    /// Forces at points (force, world point).
    at: Vec<(Vec3, Vec3)>,
    /// Whether the push wakes the body: holding still doesn't.
    wake: bool,
}

impl Mover {
    /// The bodies this mover reads.
    pub(crate) fn bodies(&self) -> impl Iterator<Item = RigidBodyHandle> {
        let other = match &self.kind {
            Kind::Force { relative, .. } | Kind::Torque { relative, .. } => relative.map(|f| f.0),
            Kind::Spring { other, .. } => Some(other.0),
            _ => None,
        };
        std::iter::once(self.at.0).chain(other)
    }

    /// Whether the mover moves its body `s` on its own, rather than
    /// holding it where it is.
    fn asks_motion(&self, s: &State) -> bool {
        match &self.kind {
            Kind::Velocity { target, .. } | Kind::AngularVelocity { target, .. } => {
                *target != Vec3::ZERO
            }
            Kind::Rocket { target, radius, .. } => {
                (s.pose * self.at.1).position.distance(*target) > *radius
            }
            _ => false,
        }
    }
}

/// Apply every mover's forces for one step of `h` seconds, replacing last
/// step's. `forced` holds the bodies pushed last step.
pub(crate) fn apply<'a>(
    world: &mut PhysicsWorld,
    movers: impl Iterator<Item = &'a Mover> + Clone,
    h: f32,
    forced: &mut Vec<RigidBodyHandle>,
) {
    profiling::scope!("movers::apply");
    for b in forced.drain(..) {
        if let Some(rb) = world.bodies.get_mut(b) {
            rb.reset_forces(false);
            rb.reset_torques(false);
        }
    }
    let gravity = to_glam(world.gravity);
    let mut states: HashMap<RigidBodyHandle, State> = HashMap::default();
    for b in movers.clone().flat_map(Mover::bodies) {
        if !states.contains_key(&b)
            && let Some(rb) = world.bodies.get(b)
        {
            states.insert(b, State::of(rb));
        }
    }
    let rotation_of = |rel: Option<Frame>| match rel {
        Some((b, f)) => states.get(&b).map(|s| (s.pose * f).rotation),
        None => Some(Mat3::IDENTITY),
    };
    let mut pushes: HashMap<RigidBodyHandle, Push> = HashMap::default();
    for m in movers {
        let (body, frame) = m.at;
        let Some(s) = states.get(&body) else {
            continue;
        };
        // A spring between an anchored part and a free one still pulls
        // the free one.
        if !s.dynamic && !matches!(m.kind, Kind::Spring { .. }) {
            continue;
        }
        let world_frame = s.pose * frame;
        // A mover wakes its body when it asks for motion, or when a body
        // it reads moves.
        let moved = |b: RigidBodyHandle| states.get(&b).is_some_and(|o| o.awake);
        pushes.entry(body).or_default().wake |=
            m.asks_motion(s) || m.bodies().any(|b| b != body && moved(b));
        match &m.kind {
            Kind::Force {
                force,
                relative,
                point,
            } => {
                let Some(r) = rotation_of(*relative) else {
                    continue;
                };
                let f = r * *force;
                match point {
                    Some(p) => pushes
                        .entry(body)
                        .or_default()
                        .at
                        .push((f, world_frame.position + world_frame.rotation * *p)),
                    None => pushes.entry(body).or_default().force += f,
                }
            }
            Kind::Torque { torque, relative } => {
                if let Some(r) = rotation_of(*relative) {
                    pushes.entry(body).or_default().torque += r * *torque;
                }
            }
            Kind::Velocity { target, max_force } => {
                let a = (*target - s.v) / h - gravity;
                pushes.entry(body).or_default().force += clamp_axes(s.mass * a, *max_force);
            }
            Kind::AngularVelocity { target, max_torque } => {
                let alpha = (*target - s.w) / h;
                pushes.entry(body).or_default().torque +=
                    clamp_axes(s.torque_for(alpha), *max_torque);
            }
            Kind::Position {
                target,
                p,
                d,
                max_force,
            } => {
                let err = *target - world_frame.position;
                let a = spring_accel(err, s.v, *p, *d, h, gravity);
                pushes.entry(body).or_default().force += clamp_axes(s.mass * a, *max_force);
            }
            Kind::Gyro {
                target,
                p,
                d,
                max_accel,
            } => {
                let err = rotation_error(world_frame.rotation, *target);
                let alpha = clamp_axes(gyro_accel(err, s.w, *p, *d, h), *max_accel);
                pushes.entry(body).or_default().torque += s.torque_for(alpha);
            }
            Kind::Rocket {
                target,
                max_speed,
                max_thrust,
                radius,
                turn_p,
                turn_d,
                max_accel,
                cartoon,
            } => {
                let r = world_frame.rotation;
                let look = -r.z_axis;
                let to = *target - world_frame.position;
                let dist = to.length();
                let dir = if dist > 1e-6 { to / dist } else { look };
                let desired = if dist > *radius {
                    dir * *max_speed
                } else {
                    Vec3::ZERO
                };
                let thrust =
                    (s.mass * ((desired - s.v) / h - gravity)).clamp_length_max(max_thrust.abs());
                pushes.entry(body).or_default().force += thrust;
                let aim = (*cartoon * dir + (1.0 - *cartoon) * thrust.normalize_or(dir))
                    .normalize_or(dir);
                // Inference: MaxTorque's default (no Z) leaves the rocket's
                // roll free, so it limits the rocket's own axes.
                let alpha = gyro_accel(look.cross(aim), s.w, *turn_p, *turn_d, h);
                let local = clamp_axes(r.transpose() * alpha, *max_accel);
                pushes.entry(body).or_default().torque += s.torque_for(r * local);
            }
            Kind::Spring {
                other,
                stiffness,
                damping,
                free_length,
                max_force,
            } => {
                let Some(o) = states.get(&other.0) else {
                    continue;
                };
                let p0 = world_frame.position;
                let p1 = (o.pose * other.1).position;
                let d = p1 - p0;
                let len = d.length();
                if len < 1e-6 {
                    continue;
                }
                let n = d / len;
                let along = (o.velocity_at(p1) - s.velocity_at(p0)).dot(n);
                let f = (stiffness * (len - free_length) + damping * along)
                    .clamp(-max_force.abs(), max_force.abs());
                pushes.entry(body).or_default().at.push((f * n, p0));
                let o = pushes.entry(other.0).or_default();
                o.at.push((-f * n, p1));
                o.wake |= s.awake;
            }
        }
    }
    for (b, push) in pushes {
        if !states.get(&b).is_some_and(|s| s.dynamic) {
            continue;
        }
        let Some(rb) = world.bodies.get_mut(b) else {
            continue;
        };
        let wake = push.wake;
        rb.add_force(to_rapier(push.force), wake);
        rb.add_torque(to_rapier(push.torque), wake);
        for (f, p) in push.at {
            rb.add_force_at_point(to_rapier(f), to_rapier(p), wake);
        }
        forced.push(b);
    }
}

/// Where a rocket's part is relative to its target's radius, to fire
/// ReachedTarget as it arrives.
pub(crate) fn rocket_arrived(world: &PhysicsWorld, m: &Mover) -> Option<bool> {
    let Kind::Rocket { target, radius, .. } = &m.kind else {
        return None;
    };
    let rb = world.bodies.get(m.at.0)?;
    let p = (cframe_of(rb.position()) * m.at.1).position;
    Some(p.distance(*target) <= *radius)
}
