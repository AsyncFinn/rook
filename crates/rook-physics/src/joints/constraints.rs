//! Attachment-based constraints that become Rapier joints (HingeConstraint,
//! BallSocketConstraint, RopeConstraint, RodConstraint), and the motors
//! that drive revolute joints: HingeConstraint's Motor and Servo actuators
//! and the legacy RotateV and RotateP.
//!
//! Observed in Studio 0.742 unless marked otherwise. Constrained parts keep
//! colliding with each other (Inference: that's what
//! NoCollisionConstraint exists for).
//!
//! The joints are stiff but not rigid, as Roblox's are: a part saved
//! overlapping an anchored one gets pushed out a little and the
//! constraint gives, instead of the two fighting (see [`JOINT_SOFTNESS`]).

use rapier3d::prelude::*;
use rook_dom::{Dom, InstanceId};
use rook_types::CFrame;

use crate::convert::pose_of;
use crate::shape::is_part;

/// Classes this module turns into joints.
pub(crate) fn is_constraint(class: &str) -> bool {
    matches!(
        class,
        "HingeConstraint" | "BallSocketConstraint" | "RopeConstraint" | "RodConstraint"
    )
}

/// An attachment's part and its CFrame in that part.
pub(crate) fn attachment(dom: &Dom, att: InstanceId) -> Option<(InstanceId, CFrame)> {
    if dom.class_name(att) != "Attachment" {
        return None;
    }
    let part = dom.parent(att).filter(|p| is_part(dom, *p))?;
    Some((
        part,
        dom.cframe_property(att, "CFrame")
            .unwrap_or(CFrame::IDENTITY),
    ))
}

/// A constraint's two attachments, if it's enabled and has both.
pub(crate) fn attachments(dom: &Dom, id: InstanceId) -> Option<[InstanceId; 2]> {
    if dom.is_destroyed(id) || !dom.bool_property(id, "Enabled").unwrap_or(true) {
        return None;
    }
    Some([
        dom.ref_property(id, "Attachment0")?,
        dom.ref_property(id, "Attachment1")?,
    ])
}

/// The joint a constraint makes, with its frames in each attachment's part.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Spec {
    pub parts: [InstanceId; 2],
    pub frames: [CFrame; 2],
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    /// Turns about the attachments' X axes; limits in radians.
    Hinge {
        limits: Option<[f32; 2]>,
    },
    BallSocket,
    /// The attachments stay between these distances (a rope 0 to Length,
    /// a rod exactly Length).
    Distance {
        min: f32,
        max: f32,
    },
}

impl Spec {
    pub(crate) fn read(dom: &Dom, id: InstanceId) -> Option<Self> {
        let [a0, a1] = attachments(dom, id)?;
        let (p0, f0) = attachment(dom, a0)?;
        let (p1, f1) = attachment(dom, a1)?;
        let kind = match dom.class_name(id) {
            "HingeConstraint" => Kind::Hinge {
                limits: dom
                    .bool_property(id, "LimitsEnabled")
                    .unwrap_or(false)
                    .then(|| {
                        let lower = dom
                            .float_property(id, "LowerAngle")
                            .unwrap_or(0.0)
                            .to_radians();
                        let upper = dom
                            .float_property(id, "UpperAngle")
                            .unwrap_or(0.0)
                            .to_radians();
                        [lower.min(upper), upper.max(lower)]
                    }),
            },
            "BallSocketConstraint" => Kind::BallSocket,
            "RopeConstraint" => Kind::Distance {
                min: 0.0,
                max: dom.float_property(id, "Length").unwrap_or(0.0).max(0.0),
            },
            "RodConstraint" => {
                let length = dom.float_property(id, "Length").unwrap_or(0.0).max(0.0);
                Kind::Distance {
                    min: length,
                    max: length,
                }
            }
            _ => return None,
        };
        Some(Self {
            parts: [p0, p1],
            frames: [f0, f1],
            kind,
        })
    }

    /// The Rapier joint, given each part's pose in its body.
    pub(crate) fn joint(&self, offsets: [CFrame; 2]) -> GenericJoint {
        let frame = |i: usize| pose_of(&(offsets[i] * self.frames[i]));
        let mut b = match self.kind {
            Kind::Hinge { limits } => {
                let b = GenericJointBuilder::new(JointAxesMask::LOCKED_REVOLUTE_AXES);
                match limits {
                    Some(l) => b.limits(JointAxis::AngX, l),
                    None => b,
                }
            }
            Kind::BallSocket => GenericJointBuilder::new(JointAxesMask::LOCKED_SPHERICAL_AXES),
            Kind::Distance { min, max } => GenericJointBuilder::new(JointAxesMask::empty())
                .coupled_axes(JointAxesMask::LIN_AXES)
                .limits(JointAxis::LinX, [min, max]),
        };
        b = b.local_frame1(frame(0)).local_frame2(frame(1));
        b.softness(JOINT_SOFTNESS).build()
    }
}

/// How firmly attachment constraints hold. Observed: a block hung by a
/// HingeConstraint from an anchored part, saved overlapping an anchored
/// floor by 1 stud, rests still with the hinge stretched 0.05 studs; a
/// laser's spinner whose union rests on its anchored beam, 1.72 studs.
/// Rapier's rigid default (1 MHz) instead fights the contacts every step
/// until the solver diverges and throws the assembly about. This gives
/// 0.07 studs in the first case (Approximation: Rapier pushes out at a
/// capped speed, so the give doesn't grow with the overlap as Studio's
/// does).
const JOINT_SOFTNESS: SpringCoefficients<Real> = SpringCoefficients {
    natural_frequency: 100.0,
    damping_ratio: 10.0,
};

/// What turns a revolute joint about its axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Drive {
    /// Turns freely.
    Free,
    /// Turns at `speed` rad/s (RotateV; HingeConstraint's Motor).
    Speed { speed: f32, max_torque: f32 },
    /// Turns toward `target` at up to `speed` rad/s, slowing near it
    /// (HingeConstraint's Servo).
    Servo {
        target: f32,
        speed: f32,
        max_torque: f32,
    },
    /// Holds `angle` (RotateP, which moves its angle in steps).
    Angle { angle: f32 },
}

/// A servo slows down this close to its target: speed = gain × error
/// (Approximation of the ease-in observed in Studio, which took about
/// 0.1 s to close the last 9 degrees).
const SERVO_GAIN: f32 = 20.0;

/// How stiff motor constraints are: Rapier's acceleration-based motor
/// gains, high enough that motors reach their speed or angle within a step
/// or two, as Roblox's do (Approximation).
const MOTOR_DAMPING: f32 = 1.0e4;
const ANGLE_STIFFNESS: f32 = 1.0e6;
const ANGLE_DAMPING: f32 = 1.0e3;

/// HingeConstraint's actuator (`Enum.ActuatorType`: None 0, Motor 1,
/// Servo 2). Angles are in degrees on the instance.
pub(crate) fn hinge_constraint_drive(dom: &Dom, id: InstanceId) -> Drive {
    match dom.enum_property(id, "ActuatorType").unwrap_or(0) {
        1 => Drive::Speed {
            speed: dom.float_property(id, "AngularVelocity").unwrap_or(0.0),
            max_torque: dom.float_property(id, "MotorMaxTorque").unwrap_or(0.0),
        },
        2 => Drive::Servo {
            target: dom
                .float_property(id, "TargetAngle")
                .unwrap_or(0.0)
                .to_radians(),
            speed: dom.float_property(id, "AngularSpeed").unwrap_or(0.0).abs(),
            max_torque: dom.float_property(id, "ServoMaxTorque").unwrap_or(0.0),
        },
        _ => Drive::Free,
    }
}

/// `joint` with its motor set for `drive`, given its current angle, or
/// `None` when the motor is already set that way.
pub(crate) fn apply_drive(joint: &GenericJoint, drive: Drive, angle: f32) -> Option<GenericJoint> {
    let mut j = *joint;
    match drive {
        Drive::Free => {
            j.set_motor(JointAxis::AngX, 0.0, 0.0, 0.0, 0.0);
            j.set_motor_max_force(JointAxis::AngX, 0.0);
        }
        Drive::Speed { speed, max_torque } => {
            j.set_motor(JointAxis::AngX, 0.0, speed, 0.0, MOTOR_DAMPING);
            j.set_motor_max_force(JointAxis::AngX, max_torque.max(0.0));
        }
        Drive::Servo {
            target,
            speed,
            max_torque,
        } => {
            let v = (SERVO_GAIN * (target - angle)).clamp(-speed, speed);
            j.set_motor(JointAxis::AngX, 0.0, v, 0.0, MOTOR_DAMPING);
            j.set_motor_max_force(JointAxis::AngX, max_torque.max(0.0));
        }
        Drive::Angle { angle } => {
            j.set_motor(JointAxis::AngX, angle, 0.0, ANGLE_STIFFNESS, ANGLE_DAMPING);
            j.set_motor_max_force(JointAxis::AngX, Real::MAX);
        }
    }
    (j.motors != joint.motors || j.motor_axes != joint.motor_axes).then_some(j)
}

/// A revolute joint's angle about its axis (X of its frames): Part1's
/// turn relative to Part0, positive by the right-hand rule.
pub(crate) fn revolute_angle(joint: &GenericJoint, pose1: &Pose, pose2: &Pose) -> f32 {
    let r1 = pose1.rotation * joint.local_frame1.rotation;
    let r2 = pose2.rotation * joint.local_frame2.rotation;
    let rel = r1.inverse() * r2;
    let angle = rel.x.clamp(-1.0, 1.0).asin() * 2.0;
    if rel.w < 0.0 { -angle } else { angle }
}
