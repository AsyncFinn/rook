use super::*;
use crate::assembly::bodies::SLEEP_ACCELERATION;

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
        if !self.simulates_anything() {
            self.play_only(steps, h);
            return;
        }
        self.playback.only = false;
        self.awake.clear();
        self.awake.extend(self.world.islands.active_bodies());
        self.build_terrain_near_bodies(dt as f32);
        self.drive_joints(dt, steps);
        let events = contacts::Collector::default();
        self.find_played_near(dt);
        for i in 0..steps {
            let last = i + 1 == steps;
            if last {
                self.remember_poses();
                self.remember_far_played();
            }
            if !self.movers.is_empty() || !self.forced.is_empty() {
                movers::apply(&mut self.world, self.movers.values(), h, &mut self.forced);
            }
            self.play_remote(h);
            self.steer_humanoids(h);
            self.before_step.clear();
            for handle in self.world.islands.active_bodies() {
                let rb = &self.world.bodies[handle];
                self.before_step.push((handle, rb.linvel(), rb.angvel()));
            }
            self.world.step_with_events(&contacts::Mixer, &events);
            self.keep_accelerating_awake(h);
            self.notice_flight(h);
            self.update_humanoids(h);
        }
        // Stepping put every collider in place, but for the far played
        // assemblies', placed after it.
        self.stale.clear();
        self.stale_statics.clear();
        self.place_far_played(h);
        let far = !self.playback.moved().is_empty();
        if far || self.world.islands.active_bodies().next().is_some() {
            self.generation += 1;
        }
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

    /// Whether a step has anything to simulate: an unanchored assembly of
    /// this world's, a server's assembly a client simulates, or a mover
    /// or push to apply. Played assemblies alone aren't: they go where
    /// their updates have them.
    fn simulates_anything(&self) -> bool {
        let touches_remote = matches!(self.authority, Authority::Except(_));
        let [local, remote] = self.moving;
        local > 0
            || (touches_remote && remote > 0)
            || !self.movers.is_empty()
            || !self.forced.is_empty()
    }

    /// A frame of `steps` steps of `h` seconds with nothing to simulate
    /// (a client without a character): no Rapier step runs, which would
    /// cost its fixed share each time for bodies that only play back. The
    /// playback clock moves on and each played assembly is put where its
    /// updates have it, as far ones always are. The colliders stay stale
    /// for queries to place.
    fn play_only(&mut self, steps: u32, h: f32) {
        self.playback.only = true;
        self.awake.clear();
        // Nothing simulated is near one: every played assembly sleeps.
        self.find_played_near(f64::from(steps) / STEP_HZ);
        for i in 0..steps {
            if i + 1 == steps {
                self.before_last_step.clear();
                self.remember_far_played();
            }
            self.play_remote(h);
        }
        self.place_far_played(h);
        if !self.playback.moved().is_empty() {
            self.generation += 1;
        }
    }

    /// Keep [`Physics::flying`]: the bodies only gravity moved over the step
    /// just taken. A body on anything feels its push too, so one at rest or
    /// driving along costs nothing here, and keeps Rapier's contact reuse.
    fn notice_flight(&mut self, h: f32) {
        let bodies = &self.world.bodies;
        self.flying.retain(|&b| bodies.get(b).is_some());
        let fall = self.world.gravity * h;
        for i in 0..self.before_step.len() {
            let (handle, v, _) = self.before_step[i];
            let Some(rb) = self.world.bodies.get(handle) else {
                continue;
            };
            let felt = rb.linvel() - v - fall * rb.gravity_scale();
            // Humanoids' controllers take over the gravity of those they hold.
            let falls = rb.is_dynamic() && rb.gravity_scale() > 0.0;
            let flying = falls && felt.length() < fall.length() * 0.1;
            if flying == self.flying.contains(&handle) {
                continue;
            }
            let colliders = rb.colliders().to_vec();
            if flying {
                self.flying.insert(handle);
            } else {
                self.flying.remove(&handle);
            }
            for c in colliders {
                self.set_hooks(c);
            }
        }
    }

    /// Restart the sleep timer of each body that sped up or slowed down
    /// at [`SLEEP_ACCELERATION`] or more over the step just taken, as
    /// Roblox keeps such an assembly awake however slow it moves.
    fn keep_accelerating_awake(&mut self, h: f32) {
        let most = SLEEP_ACCELERATION * h;
        for &(handle, v, w) in &self.before_step {
            let Some(rb) = self.world.bodies.get_mut(handle) else {
                continue;
            };
            let reach = rb.mass_properties().max_extent();
            let change = (rb.linvel() - v).length() + (rb.angvel() - w).length() * reach;
            if change >= most {
                rb.activation_mut().time_since_can_sleep = 0.0;
            }
        }
    }

    /// Where the moving bodies this world simulates were before the last
    /// step, for drawing between it and the step before.
    fn remember_poses(&mut self) {
        self.before_last_step.clear();
        for h in self.world.islands.active_bodies() {
            if self.bodies.get(&h).is_some_and(|b| !b.remote) || self.playback.plays(h) {
                self.before_last_step
                    .insert(h, *self.world.bodies[h].position());
            }
        }
    }

    /// Where to draw the parts of the moving assemblies this world
    /// simulates or plays back: between their last two steps, by how far
    /// the carried time is into the next (the standard interpolation, one
    /// step behind at most). Every other part is drawn at its CFrame.
    pub fn drawn_poses(&self) -> impl Iterator<Item = (InstanceId, CFrame)> + '_ {
        let along = (self.pending * STEP_HZ).clamp(0.0, 1.0) as f32;
        self.before_last_step.iter().flat_map(move |(h, before)| {
            // One handed to another machine since is drawn where it is.
            let body = (self.bodies.get(h)).filter(|b| !b.remote || self.playback.plays(*h));
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
        self.step_motors(dom, &props);
        self.write_hinge_angles(dom, &props);
        let mut fallen = Vec::new();
        let active = self.world.islands.active_bodies();
        let far = self.playback.moved().iter().copied();
        for handle in active.filter(|h| !self.playback.far(*h)).chain(far) {
            let Some(body) = self.bodies.get(&handle) else {
                continue;
            };
            let root = cframe_of(self.world.bodies[handle].position());
            if body.remote {
                // Its machine writes it; it falls out of the world here too.
                if root.position.y < self.fallen_height {
                    fallen.extend(body.asm.members.iter().map(|m| m.part));
                }
                if !self.playback.plays(handle) {
                    continue;
                }
            } else {
                for m in &body.asm.members {
                    if (root * m.offset).position.y < self.fallen_height {
                        fallen.push(m.part);
                    }
                }
            }
            for m in &body.asm.members {
                self.bounds.mark(m.part);
            }
            rook_types::count!(PhysicsBodiesTouched, 1);
            self.write_body(dom, &props, handle);
        }
        // Gone to sleep in these steps: at rest, as Roblox reads them
        // (observed: a sleeping block's velocity reads zero).
        for handle in std::mem::take(&mut self.awake) {
            let asleep = self
                .world
                .bodies
                .get(handle)
                .is_some_and(RigidBody::is_sleeping);
            if asleep && self.bodies.get(&handle).is_some_and(|b| !b.remote) {
                self.write_body(dom, &props, handle);
            }
        }
        // Our own writes aren't changes to apply next time.
        self.cursor = dom.journal().end();
        self.props = Some(props);
        fallen
    }

    /// Write where body `h` is and how fast it moves into its parts.
    pub(crate) fn write_body(&self, dom: &mut Dom, props: &Props, h: RigidBodyHandle) {
        let (Some(body), Some(rb)) = (self.bodies.get(&h), self.world.bodies.get(h)) else {
            return;
        };
        let root = cframe_of(rb.position());
        let (linear, angular) = self.velocity_of(h);
        let linvel = Value::Vector3(Vector3(linear));
        let angvel = Value::Vector3(Vector3(angular));
        // When the root moves, its entry in the journal stands for the
        // whole assembly (readers that need the others ask
        // [`Physics::moved_with`]), so replication sends one pose per
        // assembly, as Roblox does, and readers look at one entry, not one
        // per part. A member a motor turns while the root holds still is
        // journaled itself.
        let root_moved = dom.property(body.asm.root(), props.cframe) != Some(Value::CFrame(root));
        for (i, m) in body.asm.members.iter().enumerate() {
            let cf = Value::CFrame(root * m.offset);
            if i == 0 || !root_moved {
                rook_dom::ignore_stale(dom.set_property(m.part, props.cframe, cf));
            } else {
                dom.load_property(m.part, props.cframe, cf);
            }
            // No journal reader wants a moving part's velocities: no
            // Changed fires for them, nothing draws them, and replication
            // sends them with the pose. Journaled, they were two of every
            // three entries each frame, which every reader looked at.
            dom.load_property(m.part, props.linear_velocity, linvel.clone());
            dom.load_property(m.part, props.angular_velocity, angvel.clone());
        }
    }

    /// HingeConstraint.CurrentAngle, in degrees.
    fn write_hinge_angles(&self, dom: &mut Dom, props: &Props) {
        for &id in &self.hinge_constraints {
            let c = &self.constraints[&id];
            let (Some(j), Some(rb1), Some(rb2)) = (
                self.world.impulse_joints.get(c.joint),
                self.world.bodies.get(c.bodies[0]),
                self.world.bodies.get(c.bodies[1]),
            ) else {
                continue;
            };
            let angle = revolute_angle(&j.data, rb1.position(), rb2.position());
            rook_dom::ignore_stale(dom.set_property(
                id,
                props.hinge_angle,
                Value::Float32(angle.to_degrees()),
            ));
        }
    }

    fn step_motors(&mut self, dom: &mut Dom, props: &Props) {
        let dt = self.last_dt;
        let turned: Vec<RigidBodyHandle> = self
            .motored
            .iter()
            .filter(|h| {
                // Every motor steps, even after one has turned.
                self.bodies[*h].motors.iter().fold(false, |any, j| {
                    joints::drives::step_motor(dom, props, j, dt) | any
                })
            })
            .copied()
            .collect();
        for h in turned {
            self.refresh_offsets(dom, h);
        }
    }
}
