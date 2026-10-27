//! Stepping the simulation and writing the results back.

use super::*;

impl Physics {
    /// Advance the simulation by `dt` seconds in whole 240 Hz steps, the
    /// rest carried to the next call: the standard fixed-timestep loop, so
    /// what happens doesn't depend on the frame rate. Measured: scripts
    /// read parts moved by whole steps (3, 4 or 5 in frames of uneven
    /// length), so the DOM gets the stepped state; what's drawn between
    /// steps is [`Physics::drawn_poses`].
    pub fn step(&mut self, dt: f64) {
        profiling::scope!("Physics::step");
        self.last_dt = dt;
        if !self.moves_anything() {
            self.pending = 0.0;
            self.before_last_step.clear();
            return;
        }
        let due = self.pending + dt;
        let steps = (due * STEP_HZ + 1e-6).floor() as u32;
        self.pending = (due - f64::from(steps) / STEP_HZ).max(0.0);
        if steps == 0 {
            return;
        }
        let dt = f64::from(steps) / STEP_HZ;
        let h = self.world.integration_parameters.dt;
        self.build_terrain_near_bodies(dt as f32);
        self.drive_joints(dt, steps);
        let events = contacts::Collector::default();
        for i in 0..steps {
            if i + 1 == steps {
                self.remember_poses();
            }
            if !self.movers.is_empty() || !self.forced.is_empty() {
                movers::apply(&mut self.world, self.movers.values(), h, &mut self.forced);
            }
            self.steer_humanoids(h);
            self.world.step_with_events(&contacts::Mixer, &events);
        }
        if self.world.islands.active_bodies().next().is_some() {
            self.generation += 1;
        }
        self.update_humanoids(dt as f32);
        // Stepping put every collider in place.
        self.stale.clear();
        self.stale_statics.clear();
        for m in self.movers.values() {
            if let Some(inside) = movers::rocket_arrived(&self.world, m)
                && let Some(was) = self.rockets.get_mut(&m.id)
            {
                if inside && !*was {
                    self.reached.push(m.id);
                }
                *was = inside;
            }
        }
        let parts = &self.parts;
        self.touch
            .record(events.take(), &self.colliders, |p| parts.contains_key(&p));
    }

    /// Where the moving bodies this world simulates were before the last
    /// step, for drawing between it and the step before.
    fn remember_poses(&mut self) {
        self.before_last_step.clear();
        for h in self.world.islands.active_bodies() {
            if self.bodies.get(&h).is_some_and(|b| !b.remote) {
                self.before_last_step
                    .insert(h, *self.world.bodies[h].position());
            }
        }
    }

    /// Where to draw the parts of the moving assemblies this world
    /// simulates: between their last two steps, by how far the carried
    /// time is into the next (the standard interpolation, one step behind
    /// at most). Every other part is drawn at its CFrame.
    pub fn drawn_poses(&self) -> impl Iterator<Item = (InstanceId, CFrame)> + '_ {
        let along = (self.pending * STEP_HZ).clamp(0.0, 1.0) as f32;
        self.before_last_step.iter().flat_map(move |(h, before)| {
            // One handed to another machine since is drawn where it is.
            let body = self.bodies.get(h).filter(|b| !b.remote);
            let root = self
                .world
                .bodies
                .get(*h)
                .map(|rb| between(before, rb.position(), along));
            let members = body
                .zip(root)
                .map(|(b, root)| (b.asm.members.iter()).map(move |m| (m.part, root * m.offset)));
            members.into_iter().flatten()
        })
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
        self.write_hinge_angles(dom);
        let mut fallen = Vec::new();
        for handle in self.world.islands.active_bodies() {
            let Some(body) = self.bodies.get(&handle) else {
                continue;
            };
            let rb = &self.world.bodies[handle];
            let root = cframe_of(rb.position());
            if body.remote {
                // Its machine writes it; it falls out of the world here too.
                if root.position.y < self.fallen_height {
                    fallen.extend(body.asm.members.iter().map(|m| m.part));
                }
                continue;
            }
            let linvel = Value::Vector3(Vector3(to_glam(rb.linvel())));
            let angvel = Value::Vector3(Vector3(to_glam(rb.angvel())));
            for m in &body.asm.members {
                let cf = root * m.offset;
                if cf.position.y < self.fallen_height {
                    fallen.push(m.part);
                }
                rook_dom::ignore_stale(dom.set_property(m.part, props.cframe, Value::CFrame(cf)));
                rook_dom::ignore_stale(dom.set_property(
                    m.part,
                    props.linear_velocity,
                    linvel.clone(),
                ));
                rook_dom::ignore_stale(dom.set_property(
                    m.part,
                    props.angular_velocity,
                    angvel.clone(),
                ));
            }
        }
        // Our own writes aren't changes to apply next time.
        self.cursor = dom.journal().end();
        self.props = Some(props);
        fallen
    }

    /// HingeConstraint.CurrentAngle, in degrees.
    fn write_hinge_angles(&self, dom: &mut Dom) {
        for (&id, c) in &self.constraints {
            if !matches!(c.spec.kind, constraints::Kind::Hinge { .. }) {
                continue;
            }
            let (Some(j), Some(rb1), Some(rb2)) = (
                self.world.impulse_joints.get(c.joint),
                self.world.bodies.get(c.bodies[0]),
                self.world.bodies.get(c.bodies[1]),
            ) else {
                continue;
            };
            let angle = revolute_angle(&j.data, rb1.position(), rb2.position());
            rook_dom::ignore_stale(dom.set_property_by_name(
                id,
                "CurrentAngle",
                Value::Float32(angle.to_degrees()),
            ));
        }
    }

    fn step_motors(&mut self, dom: &mut Dom) {
        let dt = self.last_dt;
        let turned: Vec<RigidBodyHandle> = self
            .motored
            .iter()
            .filter(|h| {
                // Every motor steps, even after one has turned.
                self.bodies[*h]
                    .motors
                    .iter()
                    .fold(false, |any, j| joints::drives::step_motor(dom, j, dt) | any)
            })
            .copied()
            .collect();
        for h in turned {
            self.refresh_offsets(dom, h);
        }
    }
}
