//! WorldModels' queries (docs/behavior/physics/queries.md#worldmodels):
//! each WorldModel is a world of its own. Its `Raycast`, shape casts and
//! overlaps find only the parts whose nearest WorldRoot it is (not those
//! of a WorldModel inside it), nothing simulates them, and Workspace's
//! queries never see them (`shape::in_workspace_world`).
//!
//! A WorldModel's parts and their shapes are gathered when it's first
//! queried and kept until something in it changes, as a scene keeps its
//! query structure between queries: the journal Physics reads anyway
//! marks the WorldModel a change is in, and only that one is gathered
//! again. A WorldModel holds a model or two for a ViewportFrame, so its
//! queries test its parts' boxes one by one.

use rustc_hash::FxHashMap;

use glam::Vec3;
use rapier3d::parry::bounding_volume::{Aabb, BoundingVolume};
use rapier3d::parry::query::{self as parry, ShapeCastOptions};
use rapier3d::prelude::*;
use rook_dom::{Change, Dom, InstanceId};
use rook_types::CFrame;

use super::filter::Accepting;
use super::{CastShape, Filter, Hit, MAX_RAY_LENGTH, OVERLAP, Region, surface_normal};
use crate::Physics;
use crate::convert::pose_of;
use crate::shape::{self, is_part, is_world_model};

/// The WorldModels queried so far, with their parts as last gathered
/// (`None` once something in one changed).
#[derive(Debug, Default)]
pub(crate) struct WorldModels {
    worlds: FxHashMap<InstanceId, Option<Vec<Placed>>>,
}

#[derive(Debug)]
struct Placed {
    part: InstanceId,
    shape: SharedShape,
    pose: Pose,
    aabb: Aabb,
}

impl WorldModels {
    /// Mark the WorldModels `change` touches as changed: the one an
    /// instance is in, those a moved instance came from and went to, and
    /// a WorldModel moved itself; forget freed ones.
    pub(crate) fn note(&mut self, dom: &Dom, change: &Change) {
        if self.worlds.is_empty() {
            return;
        }
        match change {
            Change::Property { id, .. } => self.mark(dom, dom.ancestors(*id)),
            Change::Reparented {
                id,
                old_ancestors,
                new_ancestors,
                ..
            } => {
                self.mark(dom, old_ancestors.iter().copied());
                self.mark(dom, new_ancestors.iter().copied());
                if let Some(gathered) = self.worlds.get_mut(id) {
                    *gathered = None;
                }
            }
            Change::Collected(id) => {
                self.worlds.remove(id);
            }
            _ => {}
        }
    }

    /// Mark the nearest WorldModel among `ancestors` (nearest first), if
    /// it was queried.
    fn mark(&mut self, dom: &Dom, ancestors: impl Iterator<Item = InstanceId>) {
        for a in ancestors {
            if let Some(gathered) = self.worlds.get_mut(&a) {
                *gathered = None;
                return;
            }
            if dom.class_name(a) == "Workspace" || is_world_model(dom, a) {
                return;
            }
        }
    }
}

/// What a query in a WorldModel reads: its parts, and what decides which
/// it accepts.
struct InModel<'a> {
    parts: &'a [Placed],
    accepting: Accepting<'a>,
}

impl Physics {
    /// Bring the world up to the DOM, and gather WorldModel `model`'s
    /// parts again if anything in it changed.
    fn gather(&mut self, dom: &mut Dom, model: InstanceId) {
        self.prepare(dom);
        let gathered = self.world_models.worlds.entry(model).or_default();
        if gathered.is_some() {
            return;
        }
        let mut parts = Vec::new();
        let mut stack: Vec<InstanceId> = dom.children(model).iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            // A WorldModel inside is a world of its own (observed).
            if is_world_model(dom, id) {
                continue;
            }
            if is_part(dom, id)
                && let Some((shape, pose)) = shape::world_shape(dom, &mut self.saved, id)
            {
                let aabb = shape.compute_aabb(&pose);
                parts.push(Placed {
                    part: id,
                    shape,
                    pose,
                    aabb,
                });
            }
            stack.extend(dom.children(id).iter().rev());
        }
        *gathered = Some(parts);
    }

    /// WorldModel `model`'s parts (gathered first) and `filter` for them;
    /// `None` while it's out of the DataModel, where its queries find
    /// nothing (observed).
    fn in_model<'a>(
        &'a self,
        dom: &'a Dom,
        model: InstanceId,
        filter: &'a Filter,
    ) -> Option<InModel<'a>> {
        if !dom.is_ancestor_of(dom.root(), model) {
            return None;
        }
        let parts = self.world_models.worlds.get(&model)?.as_deref()?;
        let accepting = Accepting::new(filter, dom, self.props.as_ref()?, &self.groups);
        Some(InModel { parts, accepting })
    }

    /// [`Physics::raycast`] in WorldModel `model`.
    pub fn model_raycast(
        &mut self,
        dom: &mut Dom,
        model: InstanceId,
        origin: Vec3,
        direction: Vec3,
        filter: &Filter,
    ) -> Option<Hit> {
        profiling::scope!("Physics::model_raycast");
        self.gather(dom, model);
        let dom = &*dom;
        let length = direction.length();
        if !length.is_finite() || length <= 0.0 || !origin.is_finite() {
            return None;
        }
        let dir = direction / length;
        let max = length.min(MAX_RAY_LENGTH);
        let ray = Ray::new(origin, dir);
        let m = self.in_model(dom, model, filter)?;
        let mut best: Option<(&Placed, RayIntersection)> = None;
        for p in m.parts {
            let reach = best.as_ref().map_or(max, |(_, hit)| hit.time_of_impact);
            if p.aabb.cast_local_ray(&ray, reach, true).is_none() || !m.accepting.accepts(p.part) {
                continue;
            }
            // A part the ray starts inside (or on) is skipped, as in
            // Workspace.
            if (p.shape)
                .cast_ray(&p.pose, &ray, max, true)
                .is_some_and(|t| t <= 0.0)
            {
                continue;
            }
            if let Some(hit) = p.shape.cast_ray_and_get_normal(&p.pose, &ray, reach, true) {
                best = Some((p, hit));
            }
        }
        let (p, hit) = best?;
        let distance = hit.time_of_impact;
        Some(Hit {
            part: p.part,
            position: origin + dir * distance,
            normal: surface_normal(hit.normal),
            distance,
            material: dom.enum_property(p.part, "Material").unwrap_or(0),
        })
    }

    /// [`Physics::shapecast`] in WorldModel `model`.
    pub fn model_shapecast(
        &mut self,
        dom: &mut Dom,
        model: InstanceId,
        cast: CastShape,
        cframe: CFrame,
        direction: Vec3,
        filter: &Filter,
    ) -> Option<Hit> {
        profiling::scope!("Physics::model_shapecast");
        self.gather(dom, model);
        let dom = &*dom;
        let length = direction.length();
        if !length.is_finite() || length <= 0.0 {
            return None;
        }
        let cast = cast.shape();
        let start = pose_of(&cframe);
        let moved = Pose::from_translation(direction) * start;
        let swept = cast.compute_aabb(&start).merged(&cast.compute_aabb(&moved));
        let dir = direction / length;
        let options = ShapeCastOptions {
            max_time_of_impact: length,
            target_distance: 0.0,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        };
        let m = self.in_model(dom, model, filter)?;
        let mut best: Option<(&Placed, parry::ShapeCastHit)> = None;
        for p in m.parts {
            if !p.aabb.intersects(&swept) || !m.accepting.accepts(p.part) {
                continue;
            }
            // Parts the shape starts in are skipped, as in Workspace.
            if parry::intersection_test(&start, &*cast, &p.pose, &*p.shape)
                .is_ok_and(|t| t.intersecting)
            {
                continue;
            }
            let hit = parry::cast_shapes(
                &p.pose,
                Vector::ZERO,
                &*p.shape,
                &start,
                dir,
                &*cast,
                options,
            );
            if let Ok(Some(hit)) = hit
                && best
                    .as_ref()
                    .is_none_or(|(_, b)| hit.time_of_impact < b.time_of_impact)
            {
                best = Some((p, hit));
            }
        }
        let (p, hit) = best?;
        // witness1 and normal1 are on the part, in its own axes.
        Some(Hit {
            part: p.part,
            position: p.pose * hit.witness1,
            normal: surface_normal(p.pose.rotation * hit.normal1),
            distance: hit.time_of_impact,
            material: dom.enum_property(p.part, "Material").unwrap_or(0),
        })
    }

    /// [`Physics::parts_in_bounds`] in WorldModel `model`.
    pub fn model_parts_in_bounds(
        &mut self,
        dom: &mut Dom,
        model: InstanceId,
        region: Region,
        filter: &Filter,
        max: usize,
    ) -> Vec<InstanceId> {
        profiling::scope!("Physics::model_parts_in_bounds");
        self.gather(dom, model);
        let dom = &*dom;
        let (shape, pose) = region.shape();
        let aabb = shape.compute_aabb(&pose);
        let Some(m) = self.in_model(dom, model, filter) else {
            return Vec::new();
        };
        let found = (m.parts.iter())
            .filter(|p| {
                m.accepting.accepts(p.part)
                    && shape::world_box(dom, p.part).is_some_and(|(b, at)| {
                        b.compute_aabb(&at).intersects(&aabb)
                            && parry::intersection_test(&pose, &*shape, &at, &*b)
                                .is_ok_and(|t| t.intersecting)
                    })
            })
            .map(|p| p.part);
        sorted(found, max)
    }

    /// [`Physics::parts_in_part`] in WorldModel `model`.
    pub fn model_parts_in_part(
        &mut self,
        dom: &mut Dom,
        model: InstanceId,
        part: InstanceId,
        filter: &Filter,
        max: usize,
    ) -> Vec<InstanceId> {
        profiling::scope!("Physics::model_parts_in_part");
        self.gather(dom, model);
        let Some((shape, pose)) = shape::world_shape(dom, &mut self.saved, part) else {
            return Vec::new();
        };
        let dom = &*dom;
        let aabb = shape.compute_aabb(&pose);
        let Some(m) = self.in_model(dom, model, filter) else {
            return Vec::new();
        };
        let found = (m.parts.iter())
            .filter(|p| {
                p.part != part
                    && p.aabb.intersects(&aabb)
                    && m.accepting.accepts(p.part)
                    && parry::contact(&pose, &*shape, &p.pose, &*p.shape, 0.0)
                        .ok()
                        .flatten()
                        .is_some_and(|c| c.dist < -OVERLAP)
            })
            .map(|p| p.part);
        sorted(found, max)
    }
}

/// `found` in id order, as Workspace's overlaps are, at most `max` (0:
/// all).
fn sorted(found: impl Iterator<Item = InstanceId>, max: usize) -> Vec<InstanceId> {
    let mut found: Vec<InstanceId> = found.collect();
    found.sort_unstable();
    if max > 0 {
        found.truncate(max);
    }
    found
}
