//! Hinges, movers, attachment constraints and the motors that drive them.

use crate::*;

/// What sets a driven revolute joint's motor.
#[derive(Debug, Clone, Copy)]
pub(crate) enum DriveSource {
    /// RotateV or RotateP.
    Surface(HingeInput),
    /// A HingeConstraint's actuator.
    Actuator(Drive),
}

/// A revolute joint with a motor.
#[derive(Debug)]
pub(crate) struct Driven {
    pub(crate) joint: ImpulseJointHandle,
    pub(crate) source: DriveSource,
    /// How long a Sin input has run.
    time: f64,
    /// RotateP's angle, which steps toward its input; `None` until the
    /// joint's angle is first read.
    angle: Option<f32>,
}

/// An attachment constraint in the world.
#[derive(Debug)]
pub(crate) struct BuiltConstraint {
    pub(crate) spec: Spec,
    pub(crate) bodies: [RigidBodyHandle; 2],
    offsets: [CFrame; 2],
    pub(crate) joint: ImpulseJointHandle,
}

/// Rapier's revolute joints turn about their frames' X axis; a Roblox
/// hinge turns about its C0/C1 Z axis.
fn hinge_pose(cf: CFrame) -> Pose {
    pose_of(&cf) * Pose::from_rotation(Rotation::from_rotation_y(-std::f32::consts::FRAC_PI_2))
}

impl Physics {
    /// Join two bodies with a hinge (Rotate, RotateP, RotateV).
    pub(crate) fn add_hinge(&mut self, dom: &Dom, id: InstanceId) {
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
        if let Some(input) = hinge_input(dom, &j) {
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
    pub(crate) fn notice(&mut self, dom: &Dom, id: InstanceId) {
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

    /// Re-read the movers that changed (`all` after a rebuild), and every
    /// fired rocket, whose target may have moved. A mover that changed
    /// wakes the bodies it acts on.
    pub(crate) fn refresh_movers(&mut self, dom: &Dom, all: bool, dirty: &HashSet<InstanceId>) {
        profiling::scope!("Physics::refresh_movers");
        let ids: Vec<InstanceId> = if all {
            self.mover_ids.iter().copied().collect()
        } else {
            let changed = dirty.iter().filter(|i| self.mover_ids.contains(i));
            changed.chain(self.rockets.keys()).copied().collect()
        };
        let ws = dom.find_service("Workspace");
        for id in ids {
            let destroyed = dom.is_destroyed(id);
            if destroyed {
                self.mover_ids.remove(&id);
                self.rockets.remove(&id);
            }
            let fired = self.rockets.contains_key(&id);
            let mover = ws
                .filter(|ws| !destroyed && dom.is_ancestor_of(*ws, id))
                .and_then(|_| Mover::read(dom, id, fired, |p| self.frame_of(p)));
            let old = self.movers.remove(&id);
            if old != mover {
                let bodies: Vec<_> = old.iter().chain(&mover).flat_map(Mover::bodies).collect();
                for b in bodies {
                    self.world.wake_up(b, true);
                }
            }
            if let Some(m) = mover {
                self.movers.insert(id, m);
            }
        }
    }

    /// Build, rebuild or remove the joints of constraints that changed.
    pub(crate) fn refresh_constraints(
        &mut self,
        dom: &Dom,
        all: bool,
        dirty: &HashSet<InstanceId>,
    ) {
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
    pub(crate) fn drive_joints(&mut self, dt: f64, steps: u32) {
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
                    d.time = if input.input == SurfaceInput::Sin {
                        d.time + dt
                    } else {
                        0.0
                    };
                    let value = input.value(d.time);
                    if input.stepping {
                        let held = *d.angle.get_or_insert(angle);
                        let max = ROTATE_P_STEP * steps as f32;
                        let next = value.map_or(held, |v| held + (v - held).clamp(-max, max));
                        d.angle = Some(next);
                        Drive::Angle { angle: next }
                    } else {
                        // NoInput holds the hinge against anything.
                        Drive::Speed {
                            speed: ROTATE_V_SPEED * value.unwrap_or(0.0),
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
}

/// `Enum.InputType`: how a motor surface drives its hinge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SurfaceInput {
    /// Holds still (observed: an impulse doesn't turn it).
    NoInput,
    /// ParamB.
    Constant,
    /// `ParamA · sin(ParamB · t)`, t counted from when the input became
    /// Sin (observed).
    Sin,
}

/// What drives a RotateV or RotateP: the inputs of Part0's surface that
/// the joint sits on, the face C0's Z axis points out of (Inference: for
/// joints MakeJoints creates, that's the Motor or SteppingMotor surface,
/// as Part0 is the part whose surface decided the joint). Changes apply
/// live (observed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HingeInput {
    /// RotateP (a stepping motor, driven to an angle) rather than RotateV
    /// (driven at a speed).
    pub(crate) stepping: bool,
    pub(crate) input: SurfaceInput,
    pub(crate) param_a: f32,
    pub(crate) param_b: f32,
    /// RotateV's torque limit per unit of input ([`rotate_v_strength`]).
    pub(crate) strength: f32,
}

impl HingeInput {
    /// The input's value `t` seconds after it became Sin: RotateV's speed
    /// in units of [`ROTATE_V_SPEED`], or RotateP's angle in radians.
    /// `None` holds the hinge where it is.
    pub(crate) fn value(&self, t: f64) -> Option<f32> {
        match self.input {
            SurfaceInput::NoInput => None,
            SurfaceInput::Constant => Some(self.param_b),
            SurfaceInput::Sin => Some(self.param_a * (f64::from(self.param_b) * t).sin() as f32),
        }
    }
}

/// RotateV turns at this many radians per second per unit of input
/// (observed: ParamB 0.1, 0.5, 1 and 2 turn at 3.1, 15.5, 31 and 62).
pub(crate) const ROTATE_V_SPEED: f32 = 31.0;

/// RotateV's torque limit per unit of input per unit of [`rotate_v_strength`]'s
/// geometric inertia (observed with a 1 and a 2 stud cube: 8.3e5 and
/// 2.7e7 at ParamB 10).
pub(crate) const ROTATE_V_TORQUE: f32 = 5.0e5;

/// RotateV's torque limit per unit of input. A rotor gains speed steadily
/// at this torque until it turns at its input's speed, so a heavy one
/// spins up over seconds (observed). The limit comes from the smaller of
/// the two parts, by size alone: the same with either part's density
/// changed, and the same whichever part is the small one (observed).
///
/// Inference: modelled as the part's moment of inertia about the hinge
/// axis at unit density. That is exact for 1 and 2 stud cubes and for
/// plates up to 1 stud thick (the limit grows with thickness, observed
/// from 0.2 to 2 studs), but Roblox's limit grows in steps with size:
/// cubes from 0.5 to 8 studs measured from a quarter to three and a half
/// times this.
pub(crate) fn rotate_v_strength(dom: &Dom, j: &Joint) -> f32 {
    let inertia = |part, frame: &str| {
        let size = dom
            .vector3_property(part, "Size")
            .unwrap_or(glam::Vec3::ZERO);
        let axis = dom
            .cframe_property(j.id, frame)
            .unwrap_or(CFrame::IDENTITY)
            .rotation
            .z_axis;
        let volume = size.x * size.y * size.z;
        let sq = size * size;
        let diagonal = glam::Vec3::new(sq.y + sq.z, sq.x + sq.z, sq.x + sq.y) * (volume / 12.0);
        (axis * axis).dot(diagonal)
    };
    ROTATE_V_TORQUE * inertia(j.part0, "C0").min(inertia(j.part1, "C1"))
}

/// RotateP moves its angle this many radians per 240 Hz step toward the
/// input (observed: 2.4 rad/s, in 0.01 rad steps, whatever the distance).
pub(crate) const ROTATE_P_STEP: f32 = 0.01;

/// The input driving `j` if it's a RotateV or RotateP.
pub(crate) fn hinge_input(dom: &Dom, j: &Joint) -> Option<HingeInput> {
    let stepping = match dom.class_name(j.id) {
        "RotateV" => false,
        "RotateP" => true,
        _ => return None,
    };
    let z = dom
        .cframe_property(j.id, "C0")
        .unwrap_or(CFrame::IDENTITY)
        .rotation
        .z_axis;
    let a = z.abs();
    let face = if a.x >= a.y && a.x >= a.z {
        if z.x > 0.0 { "Right" } else { "Left" }
    } else if a.y >= a.z {
        if z.y > 0.0 { "Top" } else { "Bottom" }
    } else if z.z > 0.0 {
        "Back"
    } else {
        "Front"
    };
    let input = match dom.enum_item_property(j.part0, &format!("{face}SurfaceInput")) {
        Some("Constant") => SurfaceInput::Constant,
        Some("Sin") => SurfaceInput::Sin,
        _ => SurfaceInput::NoInput,
    };
    Some(HingeInput {
        stepping,
        input,
        param_a: dom
            .float_property(j.part0, &format!("{face}ParamA"))
            .unwrap_or(0.0),
        param_b: dom
            .float_property(j.part0, &format!("{face}ParamB"))
            .unwrap_or(0.0),
        strength: rotate_v_strength(dom, j),
    })
}

/// Advance a Motor's CurrentAngle toward DesiredAngle over `dt` seconds.
/// Returns whether it moved. MaxVelocity is radians per 60 Hz frame
/// (observed: 0.1 per frame).
pub(crate) fn step_motor(dom: &mut Dom, j: &Joint, dt: f64) -> bool {
    if !matches!(j.kind, JointKind::Motor | JointKind::Motor6D) {
        return false;
    }
    let current = dom.float_property(j.id, "CurrentAngle").unwrap_or(0.0);
    let desired = dom.float_property(j.id, "DesiredAngle").unwrap_or(0.0);
    if current == desired {
        return false;
    }
    let max = dom.float_property(j.id, "MaxVelocity").unwrap_or(0.0).abs() * (dt * 60.0) as f32;
    let next = current + (desired - current).clamp(-max, max);
    rook_dom::ignore_stale(dom.set_property_by_name(j.id, "CurrentAngle", Value::Float32(next)));
    next != current
}
