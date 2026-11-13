use crate::*;

#[derive(Debug)]
pub(crate) struct Body {
    pub(crate) asm: Assembly,
    /// Per member: its collider and the collider's pose in the part.
    pub(crate) colliders: Vec<Option<(ColliderHandle, Pose)>>,
    /// Motors inside the assembly, stepped every frame.
    pub(crate) motors: Vec<Joint>,
    /// Another machine simulates it: the body follows where the DOM puts
    /// it ([`Authority`]).
    pub(crate) remote: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Home {
    Body(RigidBodyHandle),
    /// A [`Static`]: a collider with no body.
    Static,
}

/// An anchored part on its own (no joints, no velocity), its collider
/// with no body, in place in the world ([`Physics::is_static`]).
#[derive(Debug)]
pub(crate) struct Static {
    pub(crate) collider: Option<(ColliderHandle, Pose)>,
    pub(crate) cframe: CFrame,
}

fn box_mass(size: glam::Vec3, mass: f32) -> MassProperties {
    let size = size.max(glam::Vec3::splat(shape::MIN_PART_SIZE));
    let density = mass / (size.x * size.y * size.z);
    MassProperties::from_cuboid(density, size * 0.5)
}

fn saved_mass(saved: &rook_types::SavedMass, mass: f32) -> MassProperties {
    let density = mass / saved.volume.max(f32::EPSILON);
    let [xx, xy, xz, yy, yz, zz] = saved.inertia.map(|i| i * density);
    let inertia = glam::Mat3::from_cols_array(&[xx, xy, xz, xy, yy, yz, xz, yz, zz]);
    MassProperties::with_inertia_matrix(saved.center, mass, inertia)
}

/// Roblox puts an assembly to sleep, zeroing its velocity, once it has
/// stayed under about 0.33 studs/s for half a second (observed; a block
/// sliding at 0.32 stops, at 0.34 slides on). Rapier's rule is that its
/// farthest point (spin included, out to its bounding sphere) moves under
/// twice its threshold for that long, so the threshold is half that speed.
/// Spin is judged a little more strictly than Studio's, whose radius isn't
/// pinned down (docs/behavior/physics/sleeping.md).
const SLEEP_SPEED: f32 = 0.33;
const SLEEP_AFTER: f32 = 0.5;
/// Nor does an assembly sleep while it speeds up or slows down at this
/// many studs/s² or more, however slow it is (observed: a block pushed at
/// 0.24 sleeps, at 0.25 never does). Rapier has no such rule, so stepping
/// restarts the timer of a body accelerating this fast, its farthest
/// point's acceleration judged as its speed is.
pub(crate) const SLEEP_ACCELERATION: f32 = 0.25;

impl Physics {
    /// Rebuild the assemblies of `seeds` and of every part that shared an
    /// assembly with one. Returns the new bodies.
    pub(crate) fn rebuild(
        &mut self,
        dom: &mut Dom,
        props: &Props,
        seeds: Vec<InstanceId>,
    ) -> FxHashSet<RigidBodyHandle> {
        profiling::scope!("Physics::rebuild", &format!("{} seeds", seeds.len()));
        // A part's neighbors may change assembly with it (anchoring a part
        // splits its assembly; unanchoring one joins a neighbor's).
        let mut seeds_and_neighbors = Vec::with_capacity(seeds.len());
        for part in seeds {
            seeds_and_neighbors.push(part);
            seeds_and_neighbors
                .extend(joints::part_joints(dom, part).iter().map(|j| j.other(part)));
        }
        let mut todo = Vec::with_capacity(seeds_and_neighbors.len());
        for part in seeds_and_neighbors {
            match self.parts.get(&part) {
                Some(&Home::Body(h)) => {
                    if let Some(body) = self.remove_body(h) {
                        todo.extend(body.asm.members.iter().map(|m| m.part));
                    }
                }
                Some(Home::Static) => self.remove_static(part),
                None => {}
            }
            todo.push(part);
        }
        self.generation += 1;
        let mut touched = todo.clone();
        let mut added = FxHashSet::default();
        let mut hinges = Vec::new();
        let mut clusters = assembly::Clusters::default();
        for part in todo {
            if self.parts.contains_key(&part) {
                continue;
            }
            let asm = {
                profiling::scope!("Assembly::of");
                Assembly::of_with(dom, part, &mut clusters)
            };
            if let Some(asm) = asm {
                hinges.extend(asm.hinges.iter().map(|h| h.id));
                if let Some(h) = self.add(dom, props, asm) {
                    added.insert(h);
                }
            }
        }
        for joint in hinges {
            if !self.hinges.contains_key(&joint) {
                self.add_hinge(dom, joint);
            }
        }
        // Constraints and movers on these parts held bodies that are gone.
        for h in &added {
            touched.extend(self.bodies[h].asm.members.iter().map(|m| m.part));
        }
        rook_types::count!(PhysicsBodiesTouched, added.len());
        rook_types::count!(PhysicsCollidersTouched, touched.len());
        for part in touched {
            self.dirty.extend(self.attached.holders(part));
        }
        added
    }

    fn remove_body(&mut self, h: RigidBodyHandle) -> Option<Body> {
        profiling::scope!("Physics::remove_body");
        let body = self.bodies.remove(&h)?;
        self.motored.remove(&h);
        if !body.asm.anchored {
            self.moving[usize::from(body.remote)] -= 1;
        }
        self.playback.forget(h);
        // Removing the Rapier body removes its colliders and joints.
        self.world.remove_body(h);
        for (m, c) in body.asm.members.iter().zip(&body.colliders) {
            self.parts.remove(&m.part);
            self.bounds.remove(m.part);
            self.touch.forget(m.part);
            if let Some((collider, _)) = c {
                self.colliders.remove(collider);
                self.touch.retire(*collider, m.part);
            }
        }
        for j in body.asm.joints() {
            self.joints.remove(&j.id);
        }
        for j in &body.asm.hinges {
            self.joints.remove(&j.id);
            self.hinges.remove(&j.id);
            self.drives.remove(&j.id);
        }
        Some(body)
    }

    fn remove_static(&mut self, part: InstanceId) {
        let Some(s) = self.statics.remove(&part) else {
            return;
        };
        self.stale_statics.remove(&part);
        self.parts.remove(&part);
        self.bounds.remove(part);
        self.touch.forget(part);
        if let Some((collider, _)) = s.collider {
            self.world.remove_collider(collider);
            self.colliders.remove(&collider);
            self.touch.retire(collider, part);
        }
    }

    /// Whether `asm` is a [`Static`]: one anchored part, with
    /// no joints to anything and no velocity (a conveyor keeps its own
    /// body, which carries the velocity). A joint or a velocity added later
    /// rebuilds it onto a body of its own.
    fn is_static(dom: &Dom, props: &Props, asm: &Assembly) -> bool {
        let root = asm.root();
        let still = |p| match dom.property(root, p) {
            Some(Value::Vector3(v)) => v.0 == glam::Vec3::ZERO,
            _ => true,
        };
        asm.anchored
            && asm.members.len() == 1
            && asm.hinges.is_empty()
            && joints::part_joints(dom, root).is_empty()
            && still(props.linear_velocity)
            && still(props.angular_velocity)
    }

    /// Add `asm`; returns its body, or `None` for a [`Static`].
    fn add(&mut self, dom: &mut Dom, props: &Props, asm: Assembly) -> Option<RigidBodyHandle> {
        profiling::scope!("Physics::add");
        let root = asm.root();
        let root_cf = match dom.property(root, props.cframe) {
            Some(Value::CFrame(cf)) => cf,
            _ => CFrame::IDENTITY,
        };
        if Self::is_static(dom, props, &asm) {
            let collider = self.add_collider(dom, props, None, root, root_cf, true);
            self.parts.insert(root, Home::Static);
            self.stale_statics.insert(root);
            self.bounds.add(root);
            self.statics.insert(
                root,
                Static {
                    collider,
                    cframe: root_cf,
                },
            );
            return None;
        }
        let vector = |p| match dom.property(root, p) {
            Some(Value::Vector3(v)) => v.0,
            _ => Vector::ZERO,
        };
        let remote = !asm.anchored
            && !self
                .authority
                .simulates(dom, asm.members.iter().map(|m| m.part));
        let builder = if asm.anchored {
            // An anchored part keeps its velocity and carries what touches
            // it along: a conveyor (observed).
            RigidBodyBuilder::fixed()
                .user_data(contacts::pack_velocity(vector(props.linear_velocity)))
        } else if remote {
            RigidBodyBuilder::kinematic_position_based()
        } else {
            // Rapier caps spin at a quarter turn per step (for CCD, which
            // Rook doesn't use); Roblox spins a RotateV rotor at 1,550
            // rad/s (observed).
            RigidBodyBuilder::dynamic()
                .linvel(vector(props.linear_velocity))
                .angvel(vector(props.angular_velocity))
                .allow_fast_rotation(true)
        };
        let handle = self.world.insert_body(builder.pose(pose_of(&root_cf)));
        let activation = self.world.bodies[handle].activation_mut();
        activation.normalized_linear_threshold = SLEEP_SPEED / 2.0;
        activation.time_until_sleep = SLEEP_AFTER;
        self.stale.insert(handle);
        let mut colliders = Vec::with_capacity(asm.members.len());
        for (i, m) in asm.members.iter().enumerate() {
            self.parts.insert(m.part, Home::Body(handle));
            self.bounds.add(m.part);
            colliders.push(self.add_collider(dom, props, Some(handle), m.part, m.offset, i == 0));
        }
        for j in asm.joints().chain(&asm.hinges) {
            self.joints.insert(j.id, [j.part0, j.part1]);
        }
        let motors: Vec<_> = asm
            .joints()
            .filter(|j| matches!(j.kind, JointKind::Motor | JointKind::Motor6D))
            .copied()
            .collect();
        // A remote assembly's machine turns its motors.
        if !motors.is_empty() && !remote {
            self.motored.insert(handle);
        }
        // Snap the members to where the joints hold them.
        {
            profiling::scope!("Assembly::place");
            rook_dom::ignore_stale(asm.place(dom, root_cf));
        }
        if !asm.anchored {
            self.moving[usize::from(remote)] += 1;
        }
        self.bodies.insert(
            handle,
            Body {
                asm,
                colliders,
                motors,
                remote,
            },
        );
        Some(handle)
    }

    /// What a part's collider weighs. Massless parts add no mass unless
    /// they're the root (creator-docs BasePart.Massless). A part with saved
    /// hulls has what was saved with them, or, saved without `InitialSize`,
    /// its box's (observed), whichever shape it's built with.
    fn collider_mass(&self, dom: &Dom, part: InstanceId, is_root: bool) -> ColliderMassProps {
        let mass = if !is_root && is_massless(dom, part) {
            0.0
        } else {
            self.part_mass(dom, part)
        };
        let size = dom
            .vector3_property(part, "Size")
            .unwrap_or(glam::Vec3::ONE);
        match self.saved.mass(dom, part, size) {
            Some(saved) => ColliderMassProps::MassProperties(Box::new(saved_mass(&saved, mass))),
            None if self.saved.has_hulls(dom, part) => {
                ColliderMassProps::MassProperties(Box::new(box_mass(size, mass)))
            }
            None => ColliderMassProps::Mass(mass),
        }
    }

    fn add_collider(
        &mut self,
        dom: &Dom,
        props: &Props,
        body: Option<RigidBodyHandle>,
        part: InstanceId,
        offset: CFrame,
        is_root: bool,
    ) -> Option<(ColliderHandle, Pose)> {
        // A collider with no body is fixed, placed in the world.
        let fixed = body.is_none_or(|b| self.world.bodies[b].is_fixed());
        let kinematic = body.is_some_and(|b| self.world.bodies[b].is_kinematic());
        let Some(Value::Vector3(size)) = dom.property(part, props.size) else {
            return None;
        };
        let can_collide = dom.property(part, props.can_collide) != Some(Value::Bool(false));
        // A moving part with saved geometry that doesn't collide keeps its
        // box: its hulls would be tested against everything near it every
        // step only for Touched and queries (Approximation, for speed: a
        // large modern place's spinning rings and fans made a 5-second run
        // 20% slower).
        let hulls = if can_collide || fixed {
            self.saved.shape(dom, part, size.0)
        } else {
            None
        };
        let (shape, shape_pose) = match &hulls {
            Some(s) => (s.clone(), Pose::IDENTITY),
            None => shape::collider_shape(part_shape(dom, part), size.0)?,
        };
        self.read_can_touch(dom, props, part);
        let material = materials::of_part(dom, part);
        let (solver, collision) = self.interaction_groups(dom, part);
        let collider = ColliderBuilder::new(shape);
        let collider = match self.collider_mass(dom, part, is_root) {
            ColliderMassProps::MassProperties(m) => collider.mass_properties(*m),
            ColliderMassProps::Mass(m) => collider.mass(m),
            ColliderMassProps::Density(d) => collider.density(d),
        };
        let collider = collider
            .friction(material.friction)
            .restitution(material.elasticity)
            .user_data(contacts::pack(&material))
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .solver_groups(solver)
            .collision_groups(collision)
            // Non-colliding parts still fall; they just don't push anything.
            .sensor(!can_collide)
            .position(pose_of(&offset) * shape_pose);
        // A server's Touched fires for a client's character against
        // anchored parts and other characters (observed in Studio 0.744).
        let collider = if kinematic && matches!(self.authority, Authority::Except(_)) {
            collider.active_collision_types(
                ActiveCollisionTypes::default()
                    | ActiveCollisionTypes::KINEMATIC_FIXED
                    | ActiveCollisionTypes::KINEMATIC_KINEMATIC,
            )
        } else {
            collider
        };
        let handle = self.world.insert_collider(collider, body);
        self.set_hooks(handle);
        self.colliders.insert(handle, part);
        Some((handle, shape_pose))
    }

    /// Turn [`contacts::Mixer`] on for a collider only where it changes
    /// the result.
    pub(crate) fn set_hooks(&mut self, collider: ColliderHandle) {
        let Some(c) = self.world.colliders.get(collider) else {
            return;
        };
        let body = c
            .parent()
            .and_then(|b| self.world.bodies.get(b))
            .map_or(0, |rb| rb.user_data);
        let flying = c
            .parent()
            .is_some_and(|b| self.flying.contains(&b) || self.landed.contains(&b));
        let hooks = if flying {
            ActiveHooks::MODIFY_SOLVER_CONTACTS
        } else {
            contacts::hooks_for(c.user_data, body)
        };
        if let Some(c) = self.world.colliders.get_mut(collider) {
            c.set_active_hooks(hooks);
        }
    }

    fn read_can_touch(&mut self, dom: &Dom, props: &Props, part: InstanceId) {
        if dom.property(part, props.can_touch) == Some(Value::Bool(false)) {
            self.touch.untouchable.insert(part);
        } else {
            self.touch.untouchable.remove(&part);
        }
    }

    /// A part's Material, CustomPhysicalProperties or CanTouch changed:
    /// update its collider in place, since its body and joints are the
    /// same. What rests on it reacts at once (observed in Studio 0.742: a
    /// part at rest on a 30° ramp slid as soon as its friction was set to
    /// 0), so its body and everything touching it wake up.
    pub(crate) fn refresh_surface(&mut self, dom: &Dom, props: &Props, part: InstanceId) {
        let (h, i, collider) = match self.parts.get(&part) {
            Some(&Home::Body(h)) => {
                let Some(body) = self.bodies.get(&h) else {
                    return;
                };
                let Some(i) = body.asm.members.iter().position(|m| m.part == part) else {
                    return;
                };
                (Some(h), i, body.colliders[i])
            }
            Some(Home::Static) => (None, 0, self.statics[&part].collider),
            None => return,
        };
        let Some((collider, _)) = collider else {
            return;
        };
        self.read_can_touch(dom, props, part);
        let material = materials::of_part(dom, part);
        let mass = self.collider_mass(dom, part, i == 0);
        let Some(c) = self.world.colliders.get_mut(collider) else {
            return;
        };
        match mass {
            ColliderMassProps::MassProperties(m) => c.set_mass_properties(*m),
            ColliderMassProps::Mass(m) => c.set_mass(m),
            ColliderMassProps::Density(d) => c.set_density(d),
        }
        c.set_friction(material.friction);
        c.set_restitution(material.elasticity);
        c.user_data = contacts::pack(&material);
        self.set_hooks(collider);
        // Movers read the mass before the next step would recompute it.
        if let Some(rb) = h.and_then(|h| self.world.bodies.get_mut(h)) {
            rb.recompute_mass_properties_from_colliders(&self.world.colliders);
        }
        let touching: Vec<RigidBodyHandle> = self
            .world
            .narrow_phase
            .contact_pairs_with(collider)
            .filter_map(|pair| {
                let other = if pair.collider1 == collider {
                    pair.collider2
                } else {
                    pair.collider1
                };
                self.world.colliders.get(other)?.parent()
            })
            .collect();
        if let Some(h) = h {
            self.world.wake_up(h, true);
        }
        for b in touching {
            self.world.wake_up(b, true);
        }
    }

    /// The solver and collision groups for a part's collider. Groups that
    /// don't collide don't push each other; they still touch unless
    /// Workspace.TouchesUseCollisionGroups is on (creator-docs).
    pub(crate) fn interaction_groups(
        &self,
        dom: &Dom,
        part: InstanceId,
    ) -> (InteractionGroups, InteractionGroups) {
        let id = self.groups.id_of(&groups::part_group(dom, part));
        let mask = self.groups.mask_of(id) as u32;
        let solver = InteractionGroups::new(
            Group::from_bits_truncate(1 << id),
            Group::from_bits_truncate(mask),
            InteractionTestMode::And,
        );
        let collision = if self.touches_use_groups {
            solver
        } else {
            InteractionGroups::all()
        };
        (solver, collision)
    }

    pub(crate) fn frame_of(&self, part: InstanceId) -> Option<(RigidBodyHandle, CFrame)> {
        match *self.parts.get(&part)? {
            Home::Body(h) => Some((h, self.bodies.get(&h)?.asm.offset_of(part)?)),
            // The shared body sits at the origin.
            Home::Static => Some((self.shared, self.statics.get(&part)?.cframe)),
        }
    }

    /// Re-read a body's joint offsets, move its colliders and write its
    /// parts' CFrames.
    pub(crate) fn refresh_offsets(&mut self, dom: &mut Dom, h: RigidBodyHandle) {
        let Some(body) = self.bodies.get_mut(&h) else {
            return;
        };
        body.asm.refresh_offsets(dom);
        for (m, c) in body.asm.members.iter().zip(&body.colliders) {
            if let Some((collider, shape_pose)) = c
                && let Some(col) = self.world.colliders.get_mut(*collider)
            {
                col.set_position_wrt_parent(pose_of(&m.offset) * *shape_pose);
            }
        }
        let root = cframe_of(self.world.bodies[h].position());
        rook_dom::ignore_stale(body.asm.place(dom, root));
        self.world.wake_up(h, true);
        self.body_moved(h);
    }

    /// A script moved a [`Static`] (or set its velocity): its collider goes
    /// there, or, given a velocity, it's rebuilt onto a body of its own.
    pub(crate) fn teleport_static(&mut self, dom: &mut Dom, props: &Props, part: InstanceId) {
        let Some(Value::CFrame(cf)) = dom.property(part, props.cframe) else {
            return;
        };
        let moving = [props.linear_velocity, props.angular_velocity]
            .into_iter()
            .any(|p| matches!(dom.property(part, p), Some(Value::Vector3(v)) if v.0 != glam::Vec3::ZERO));
        if moving {
            self.rebuild(dom, props, vec![part]);
            return;
        }
        let Some(s) = self.statics.get_mut(&part) else {
            return;
        };
        s.cframe = cf;
        rook_types::count!(PhysicsCollidersTouched, 1);
        if let Some((collider, shape_pose)) = s.collider
            && let Some(co) = self.world.colliders.get_mut(collider)
        {
            co.set_position(pose_of(&cf) * shape_pose);
        }
        // Queries put it in place first, as for a moved body.
        self.static_moved(part);
        // Constraints and movers hold it by its place on the shared body.
        self.place_held(part);
    }

    /// A script moved `part` (or set its velocity): move its body to match.
    pub(crate) fn teleport(
        &mut self,
        dom: &mut Dom,
        props: &Props,
        h: RigidBodyHandle,
        part: InstanceId,
    ) {
        let Some(body) = self.bodies.get(&h) else {
            return;
        };
        let Some(Value::CFrame(cf)) = dom.property(part, props.cframe) else {
            return;
        };
        let offset = body.asm.offset_of(part).unwrap_or_default();
        let root = cf * offset.inverse();
        let vector = |p| match dom.property(part, p) {
            Some(Value::Vector3(v)) => v.0,
            _ => Vector::ZERO,
        };
        rook_types::count!(PhysicsBodiesTouched, 1);
        // Drawn where it was put, not on the way there.
        self.before_last_step.remove(&h);
        let rb = &mut self.world.bodies[h];
        rb.set_position(pose_of(&root), true);
        if rb.is_dynamic() {
            rb.set_linvel(vector(props.linear_velocity), true);
            rb.set_angvel(vector(props.angular_velocity), true);
        } else if !body.remote {
            rb.user_data = contacts::pack_velocity(vector(props.linear_velocity));
        }
        // Keep the rest of the assembly with it.
        rook_dom::ignore_stale(body.asm.place_around(dom, part, cf));
        if !self.world.bodies[h].is_dynamic() {
            let colliders: Vec<ColliderHandle> =
                body.colliders.iter().flatten().map(|&(c, _)| c).collect();
            for c in colliders {
                self.set_hooks(c);
            }
        }
        self.body_moved(h);
    }
}
