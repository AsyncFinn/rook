//! Stepping the simulation and writing the results back.

use super::*;

impl Physics {
    /// Advance the simulation by `dt` seconds in fixed 240 Hz steps; what
    /// a frame leaves over a whole step carries to the next, so a client
    /// drawing at any rate simulates at the right pace (a 60 Hz frame is
    /// exactly 4 steps).
    pub fn step(&mut self, dt: f64) {
        profiling::scope!("Physics::step");
        self.last_dt = dt;
        if !self.moves_anything() {
            self.pending = 0.0;
            return;
        }
        let due = self.pending + dt;
        let steps = (due * STEP_HZ + 1e-6).floor() as u32;
        self.pending = (due - f64::from(steps) / STEP_HZ).max(0.0);
        if steps == 0 {
            return;
        }
        let dt = f64::from(steps) / STEP_HZ;
        self.build_terrain_near_bodies(dt as f32);
        self.drive_joints(dt, steps);
        let events = contacts::Collector::default();
        let h = self.world.integration_parameters.dt;
        for _ in 0..steps {
            if !self.movers.is_empty() || !self.forced.is_empty() {
                movers::apply(&mut self.world, self.movers.values(), h, &mut self.forced);
            }
            self.steer_humanoids(h);
            self.world.step_with_events(&contacts::Mixer, &events);
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
