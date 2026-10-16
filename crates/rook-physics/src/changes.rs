//! Building the world from the DOM and applying the journal to it.

pub(crate) mod props;
pub(crate) mod read;

use super::*;

impl Physics {
    pub(crate) fn build(&mut self, dom: &mut Dom, props: &Props) {
        profiling::scope!("Physics::build");
        // Built with whatever geometry unions have now.
        self.unions.reshaped.clear();
        self.terrain.stale = true;
        self.read_workspace(dom, props);
        let parts = workspace_parts(dom);
        self.rebuild(dom, props, parts);
        if let Some(ws) = dom.find_service("Workspace") {
            for id in dom.descendants(ws) {
                self.notice(dom, id);
            }
        }
        self.built = true;
    }

    pub(crate) fn apply_changes(&mut self, dom: &mut Dom, props: &Props) {
        profiling::scope!("Physics::apply_changes");
        let mut structural: Vec<InstanceId> = Vec::new();
        let mut changed_joints: Vec<InstanceId> = Vec::new();
        let mut moved_joints: Vec<InstanceId> = Vec::new();
        let mut posed: Vec<InstanceId> = Vec::new();
        let mut surfaces: Vec<InstanceId> = Vec::new();
        let mut materials: Vec<InstanceId> = Vec::new();
        let mut attachments: Vec<InstanceId> = Vec::new();
        let mut workspace_changed = false;
        let db = dom.database();
        for change in dom.journal().since(self.cursor) {
            match change {
                Change::Property { id, property } => {
                    let p = *property;
                    if Some(*id) == self.terrain.instance {
                        // Its voxels, or its collision group.
                        if Some(p) == props.smooth_grid || props.is_structural(p) {
                            self.terrain.stale = true;
                        }
                    } else if props.is_structural(p) {
                        structural.push(*id);
                    } else if props.is_surface(p) {
                        materials.push(*id);
                    } else if props.is_pose(p) {
                        posed.push(*id);
                    } else if props.is_workspace(p) {
                        workspace_changed = true;
                    } else if self.joints.contains_key(id) || is_joint(dom, *id) {
                        match db.property(p).name {
                            "Part0" | "Part1" | "Enabled" => changed_joints.push(*id),
                            "C0" | "C1" | "Transform" | "CurrentAngle" => moved_joints.push(*id),
                            _ => {}
                        }
                    } else if self.constraint_ids.contains(id) || self.mover_ids.contains(id) {
                        self.dirty.insert(*id);
                    } else if props.surface_inputs.contains(&p) {
                        if !self.drives.is_empty() && self.parts.contains_key(id) {
                            surfaces.push(*id);
                        }
                    } else if !(self.constraint_ids.is_empty() && self.mover_ids.is_empty())
                        && dom.class_name(*id) == "Attachment"
                    {
                        attachments.push(*id);
                    }
                }
                Change::Reparented { subtree, .. } => {
                    for &i in subtree {
                        if Some(i) == self.terrain.instance || dom.class_name(i) == "Terrain" {
                            self.terrain.stale = true;
                        } else if self.parts.contains_key(&i) || is_part(dom, i) {
                            structural.push(i);
                        } else if self.joints.contains_key(&i) || is_joint(dom, i) {
                            changed_joints.push(i);
                        } else if dom.class_name(i) == "Attachment" {
                            attachments.push(i);
                        } else {
                            self.notice(dom, i);
                        }
                    }
                }
                _ => {}
            }
        }
        if workspace_changed {
            self.read_workspace(dom, props);
        }
        // A hinge's frames are fixed when it's created, so moving one
        // recreates it.
        moved_joints.retain(|j| {
            let hinge = self.hinges.contains_key(j);
            if hinge {
                changed_joints.push(*j);
            }
            !hinge
        });
        for j in changed_joints {
            if let Some(old) = self.joints.get(&j) {
                structural.extend(old);
            }
            structural.extend(endpoints(dom, j).into_iter().flatten());
        }
        for att in attachments {
            for &(holder, _) in dom.referrers(att) {
                if self.constraint_ids.contains(&holder) || self.mover_ids.contains(&holder) {
                    self.dirty.insert(holder);
                }
            }
        }
        let rebuilt = if structural.is_empty() {
            HashSet::default()
        } else {
            // Bodies the movers pushed and constraints joined may be gone.
            self.all_dirty = true;
            self.rebuild(dom, props, structural)
        };
        for part in surfaces {
            for j in joints::part_joints(dom, part) {
                if let (Some(d), Some(input)) =
                    (self.drives.get_mut(&j.id), joints::hinge_input(dom, &j))
                {
                    d.source = DriveSource::Surface(input);
                }
            }
        }
        let mut refreshed = HashSet::default();
        for part in materials {
            // A rebuilt body's colliders already read the new values.
            if let Some(&h) = self.parts.get(&part)
                && !rebuilt.contains(&h)
                && refreshed.insert(part)
            {
                self.refresh_surface(dom, props, h, part);
            }
        }

        let mut done = rebuilt.clone();
        for j in moved_joints {
            let Some(&[p0, _]) = self.joints.get(&j) else {
                continue;
            };
            if let Some(&h) = self.parts.get(&p0)
                && done.insert(h)
            {
                self.refresh_offsets(dom, h);
            }
        }
        let mut posed_bodies = rebuilt;
        for part in posed {
            if let Some(&h) = self.parts.get(&part)
                && posed_bodies.insert(h)
            {
                self.teleport(dom, props, h, part);
            }
        }
    }

    /// Rebuild the unions whose geometry arrived or changed.
    pub(crate) fn reshape_unions(&mut self, dom: &mut Dom, props: &Props) {
        let mut parts = std::mem::take(&mut self.unions.reshaped);
        parts.retain(|p| self.parts.contains_key(p));
        if !parts.is_empty() {
            self.all_dirty = true;
            self.rebuild(dom, props, parts);
        }
    }

    fn read_workspace(&mut self, dom: &Dom, props: &Props) {
        let Some(ws) = dom.find_service("Workspace") else {
            return;
        };
        if let Some(Value::Float32(g)) = dom.property(ws, props.gravity) {
            self.world.gravity = Vector::new(0.0, -g, 0.0);
        }
        if let Some(Value::Float32(h)) = dom.property(ws, props.fallen_height) {
            self.fallen_height = h;
        }
        let groups = CollisionGroups::read(dom);
        let touches_use_groups =
            dom.property(ws, props.touches_use_groups) == Some(Value::Bool(true));
        if groups != self.groups || touches_use_groups != self.touches_use_groups {
            self.groups = groups;
            self.touches_use_groups = touches_use_groups;
            if let Some(t) = self.terrain.instance {
                self.terrain.groups = self.interaction_groups(dom, t);
            }
            let colliders: Vec<_> = self.colliders.iter().map(|(h, p)| (*h, *p)).collect();
            for (handle, part) in colliders {
                let (solver, collision) = self.interaction_groups(dom, part);
                if let Some(c) = self.world.colliders.get_mut(handle) {
                    c.set_solver_groups(solver);
                    c.set_collision_groups(collision);
                }
            }
        }
    }
}
