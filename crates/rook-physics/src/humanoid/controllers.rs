//! Each Humanoid's controller run against its root part's body: found as
//! Humanoids appear, driven by the client's input, stepped with the world
//! and reported back.

use super::*;

impl Physics {
    /// Humanoids in Workspace's world, in id order.
    pub fn humanoids(&mut self, dom: &Dom) -> Vec<InstanceId> {
        let ws = dom.find_service("Workspace");
        self.humanoid_ids.retain(|&h| {
            !dom.is_destroyed(h)
                && ws.is_some_and(|ws| crate::shape::in_workspace_world(dom, ws, h))
        });
        let mut ids: Vec<InstanceId> = self.humanoid_ids.iter().copied().collect();
        ids.sort_unstable();
        ids
    }

    pub(crate) fn notice_humanoid(&mut self, dom: &Dom, id: InstanceId) {
        match dom.class_name(id) {
            "Humanoid" => {
                self.humanoid_ids.insert(id);
            }
            "TrussPart" => {
                self.trusses.insert(id);
            }
            _ => {}
        }
    }

    /// The controller's state for `humanoid`, if this world drives it.
    pub fn humanoid_state(&self, humanoid: InstanceId) -> Option<State> {
        self.controllers.get(&humanoid).map(|c| c.state)
    }

    /// `Humanoid:ChangeState`: the controller moves to `state` at its next
    /// step, if the state is enabled.
    pub fn change_humanoid_state(&mut self, humanoid: InstanceId, state: State) {
        if let Some(c) = self.controllers.get_mut(&humanoid) {
            c.requested = Some(state);
        }
    }

    /// Take this frame's drives: Humanoids missing from `drives` are no
    /// longer controlled, and their bodies may turn freely again.
    pub fn drive_humanoids(&mut self, drives: Vec<Drive>) {
        let mut old = std::mem::take(&mut self.controllers);
        for drive in drives {
            let id = drive.humanoid;
            let mut c = match old.remove(&id) {
                Some(mut c) => {
                    c.drive = drive;
                    c
                }
                None => Controller::new(drive),
            };
            if let Some(r) = self.rooted(c.drive.root) {
                self.hold(r.body, Some(c.state));
                if c.floor.is_none() {
                    self.sense(&mut c, &r);
                }
            }
            self.controllers.insert(id, c);
        }
        for c in old.values() {
            if let Some(r) = self.rooted(c.drive.root) {
                self.hold(r.body, None);
            }
        }
    }

    /// The root body of `root`, when this world simulates it.
    pub(super) fn rooted(&self, root: InstanceId) -> Option<Rooted> {
        let body = self.body_of(root)?;
        let b = self.bodies.get(&body)?;
        if b.remote || b.asm.anchored {
            return None;
        }
        let offset = b.asm.members.iter().find(|m| m.part == root)?.offset;
        Some(Rooted { body, offset })
    }

    /// Hold the body as `state` does: upright states lock its tilt (it may
    /// still turn about Y) and take over its gravity; the rest leave it to
    /// physics. Rapier's locks only stop forces: velocity already about X
    /// or Z still turns the body, and its gyroscopic pass makes more out of
    /// a spin about Y whenever the mass sits off to one side, as a tool or
    /// a hat puts it. So a locked body has neither.
    pub(super) fn hold(&mut self, body: RigidBodyHandle, state: Option<State>) {
        let upright = state.is_some_and(State::upright);
        let rb = &mut self.world.bodies[body];
        let tilt = LockedAxes::ROTATION_LOCKED_X | LockedAxes::ROTATION_LOCKED_Z;
        if upright != rb.locked_axes().contains(tilt) {
            rb.set_enabled_rotations(!upright, true, !upright, true);
            rb.enable_gyroscopic_forces(!upright);
        }
        let gravity = if upright { 0.0 } else { 1.0 };
        if rb.gravity_scale() != gravity {
            rb.set_gravity_scale(gravity, true);
        }
        let spin = rb.angvel();
        if upright && (spin.x != 0.0 || spin.z != 0.0) {
            rb.set_angvel(Vector::new(0.0, spin.y, 0.0), true);
        }
    }

    pub(super) fn root_pose(&self, r: &Rooted) -> CFrame {
        cframe_of(self.world.bodies[r.body].position()) * r.offset
    }

    /// Steer every controlled body for one step of `h` seconds.
    pub(crate) fn steer_humanoids(&mut self, h: f32) {
        if self.controllers.is_empty() {
            return;
        }
        let mut controllers = std::mem::take(&mut self.controllers);
        for c in controllers.values_mut() {
            if let Some(r) = self.rooted(c.drive.root) {
                self.steer(c, &r, h);
            }
        }
        self.controllers = controllers;
    }

    /// After each step: sense the surroundings and move through the
    /// states, as Roblox's controller runs with its 240 Hz physics. Once a
    /// frame, the steps between steered by a floor up to a frame old, so
    /// over uneven ground the legs bobbed more the slower the frames.
    pub(crate) fn update_humanoids(&mut self, dt: f32) {
        if self.controllers.is_empty() {
            return;
        }
        let mut controllers = std::mem::take(&mut self.controllers);
        for c in controllers.values_mut() {
            if let Some(r) = self.rooted(c.drive.root) {
                self.sense(c, &r);
                let before = c.state;
                self.transition(c, &r, dt);
                if c.state.upright() != before.upright() {
                    self.hold(r.body, Some(c.state));
                }
                // After the states, which see how far it leaned before
                // righting takes any of that back.
                if c.state.upright() {
                    self.right(&r, dt);
                }
            }
        }
        self.controllers = controllers;
    }

    /// Sense the floor, a truss or ladder ahead and water. Seated feels
    /// none of them (measured: FloorMaterial is Air, on a seat or the
    /// floor).
    pub(super) fn sense(&self, c: &mut Controller, r: &Rooted) {
        if c.state == State::Seated {
            (c.floor, c.climb, c.water) = (None, None, None);
            return;
        }
        let root = self.root_pose(r);
        c.floor = self.sense_floor(c, r.body, &root);
        let height = c.drive.standing_height;
        c.rising &= c
            .floor
            .is_some_and(|f| f.y + height - root.position.y > states::STOOD_UP);
        c.climb = self.sense_climb(c, r.body, &root);
        c.water = self.water_surface(root.position);
    }

    /// What each Humanoid did since the last call.
    pub fn humanoid_reports(&mut self, dom: &Dom) -> Vec<Report> {
        let mut controllers = std::mem::take(&mut self.controllers);
        let reports = controllers
            .values_mut()
            .map(|c| {
                let floor_material = c
                    .floor
                    .as_ref()
                    .map(|f| self.material_of(dom, f.collider, f.part));
                Report {
                    humanoid: c.drive.humanoid,
                    changes: std::mem::take(&mut c.changes),
                    state: c.state,
                    floor_material,
                    speed: self.state_speed(c),
                    clear_jump: std::mem::take(&mut c.clear_jump),
                    clear_sit: std::mem::take(&mut c.clear_sit),
                }
            })
            .collect();
        self.controllers = controllers;
        reports
    }

    pub(super) fn state_speed(&self, c: &Controller) -> f32 {
        let Some(r) = self.rooted(c.drive.root) else {
            return 0.0;
        };
        let v = self.world.bodies[r.body].linvel();
        let floor = c.floor.as_ref().map_or(glam::Vec3::ZERO, |f| f.velocity);
        match c.state {
            State::Climbing => v.y,
            State::Swimming => v.length(),
            _ => horizontal(v - floor).length(),
        }
    }
}
