//! Which parts a region, a part or a part's own shape overlaps or touches.

use super::*;

impl Physics {
    /// Parts whose collider overlaps `shape` at `pose` by `test`, in id
    /// order (Approximation: Roblox's order is its own; parts made in
    /// order came back in that order), at most `max` of them (0: all).
    pub(crate) fn overlapping(
        &self,
        dom: &Dom,
        query: (&dyn Shape, Pose),
        filter: &Filter,
        max: usize,
        skip: Option<InstanceId>,
        test: impl Fn(InstanceId, &Collider) -> bool,
    ) -> Vec<InstanceId> {
        let (shape, pose) = query;
        let accept = self.part_filter(dom, filter);
        let aabb = shape.compute_aabb(&pose);
        let mut found: Vec<InstanceId> = self
            .world
            .query_pipeline()
            .intersect_aabb_conservative(aabb)
            .filter_map(|(h, co)| {
                if self.terrain.material(h).is_some() {
                    return None;
                }
                let part = accept(h).filter(|p| Some(*p) != skip)?;
                test(part, co).then_some(part)
            })
            .collect::<FxHashSet<_>>()
            .into_iter()
            .collect();
        found.sort_unstable();
        if max > 0 {
            found.truncate(max);
        }
        found
    }

    /// Parts whose `Size` box meets `shape` at `pose`: Region3 (an
    /// axis-aligned box), `GetPartBoundsInBox` and `GetPartBoundsInRadius`,
    /// in id order, at most `max` of them (0: all). Candidates come from
    /// the parts' boxes ([`bounds`]), so a turned cylinder or wedge is
    /// found near its box's corners too.
    pub fn parts_in_bounds(
        &mut self,
        dom: &mut Dom,
        region: Region,
        filter: &Filter,
        max: usize,
    ) -> Vec<InstanceId> {
        profiling::scope!("Physics::parts_in_bounds");
        self.prepare(dom);
        let dom = &*dom;
        let (shape, pose) = region.shape();
        let aabb = shape.compute_aabb(&pose);
        // Built from the fields it reads: the boxes below are borrowed too.
        let Some(props) = self.props.as_ref() else {
            return Vec::new();
        };
        let accepting = Accepting::new(filter, dom, props, &self.groups);
        let mut found: Vec<InstanceId> = self
            .bounds
            .near(dom, &aabb)
            .filter(|&part| {
                accepting.accepts(part)
                    && shape::world_box(dom, part).is_some_and(|(b, at)| {
                        parry::intersection_test(&pose, &*shape, &at, &*b)
                            .is_ok_and(|t| t.intersecting)
                    })
            })
            .collect();
        found.sort_unstable();
        if max > 0 {
            found.truncate(max);
        }
        found
    }

    /// `WorldRoot:GetPartsInPart`: parts whose shape overlaps `part`'s.
    /// The part needn't be in Workspace (observed).
    pub fn parts_in_part(
        &mut self,
        dom: &mut Dom,
        part: InstanceId,
        filter: &Filter,
        max: usize,
    ) -> Vec<InstanceId> {
        profiling::scope!("Physics::parts_in_part");
        self.prepare(dom);
        let Some((shape, pose)) = shape::world_shape(dom, &mut self.saved, part) else {
            return Vec::new();
        };
        self.overlapping(dom, (&*shape, pose), filter, max, Some(part), |_, co| {
            overlaps(&*shape, &pose, co)
        })
    }

    /// `BasePart:GetTouchingParts`: Workspace parts overlapping `part`,
    /// its own assembly's included. A colliding part only reports
    /// colliding parts; one with `CanCollide` off reports nothing unless
    /// `touch_listened` (something is connected to its Touched, which
    /// gives it a TouchInterest), and then every part it overlaps
    /// (observed; creator-docs says the same). `CanQuery` doesn't matter.
    /// Terrain's solid surface counts for every part (observed).
    pub fn touching_parts(
        &mut self,
        dom: &mut Dom,
        part: InstanceId,
        touch_listened: bool,
    ) -> Vec<InstanceId> {
        profiling::scope!("Physics::touching_parts");
        self.prepare(dom);
        if !self.parts.contains_key(&part) {
            return Vec::new();
        }
        let collides = |p| dom.bool_property(p, "CanCollide").unwrap_or(true);
        // Parts that touch it, if any: all of them, or only colliding ones.
        let others = if collides(part) {
            Some(false)
        } else if touch_listened {
            Some(true)
        } else {
            None
        };
        let Some((shape, pose)) = shape::world_shape(dom, &mut self.saved, part) else {
            return Vec::new();
        };
        let aabb = shape.compute_aabb(&pose);
        let center = pose.translation;
        self.build_terrain_in(aabb.mins, aabb.maxs);
        // Solid terrain counts whatever the part's CanCollide, and so does
        // a part inside it (observed); water doesn't.
        let mut buried_in = None;
        let mut found: Vec<InstanceId> = Vec::new();
        for (h, co) in self
            .world
            .query_pipeline()
            .intersect_aabb_conservative(aabb)
        {
            let Some(&other) = self.colliders.get(&h) else {
                continue;
            };
            let touching = match self.terrain.material(h) {
                Some(_) if self.terrain.is_water(h) => false,
                Some(_) => {
                    let touching = overlaps(&*shape, &pose, co);
                    if !touching {
                        buried_in = Some(other);
                    }
                    touching
                }
                None => {
                    others.is_some_and(|all| other != part && (all || collides(other)))
                        && overlaps(&*shape, &pose, co)
                }
            };
            if touching {
                found.push(other);
            }
        }
        // Only a piece of terrain around it that it doesn't touch asks
        // whether it's inside, which looks up from it.
        if let Some(terrain) = buried_in
            && self.inside_terrain(center)
        {
            found.push(terrain);
        }
        found.sort_unstable();
        found.dedup();
        found
    }
}

/// How far shapes must overlap to count as touching, in studs, so faces
/// that only meet (within float error, far from the origin) don't.
/// Inference: Studio counts 0.01 studs of overlap and not faces that meet.
pub(super) const OVERLAP: f32 = 1e-3;

/// Whether `shape` at `pose` overlaps a collider by more than [`OVERLAP`].
pub(super) fn overlaps(shape: &dyn Shape, pose: &Pose, co: &Collider) -> bool {
    let deep = |other: &dyn Shape| {
        parry::contact(pose, shape, co.position(), other, 0.0)
            .ok()
            .flatten()
            .is_some_and(|c| c.dist < -OVERLAP)
    };
    match co.shape().as_trimesh() {
        // Terrain: parry's contact query takes its triangles one at a time.
        Some(mesh) => {
            let local = shape.compute_aabb(&(co.position().inverse() * *pose));
            mesh.bvh()
                .intersect_aabb(&local)
                .any(|i| deep(&mesh.triangle(i)))
        }
        None => deep(co.shape()),
    }
}
