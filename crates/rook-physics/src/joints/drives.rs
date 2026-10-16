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
    /// fired rocket, whose target may have moved.
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
}
