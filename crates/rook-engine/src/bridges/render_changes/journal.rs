//! Reading the journal each frame into what the renderer must redraw.

use super::*;

impl DataModel {
    /// Note whether the journal since the last check touched anything drawn:
    /// parts, their meshes, decals and SurfaceAppearances, their ancestry,
    /// Lighting, the Terrain and `Workspace.GlobalWind` (which bends grass),
    /// or MaterialService and its MaterialVariants (which redraw the parts
    /// and terrain they can draw on, found in `material_parts`).
    /// Camera moves don't count; the renderer reads the camera every frame
    /// anyway.
    pub(crate) fn check_render_changes(&mut self) {
        profiling::scope!("check_render_changes");
        let calls = std::mem::take(&mut self.runtime.world_mut().effect_calls);
        self.render_changes.effect_calls.extend(calls);
        let world = self.runtime.world();
        let dom = &world.dom;
        let lighting = dom.find_service("Lighting");
        // Lighting's look is its own properties and its children's (the
        // Sky, a ColorGradingEffect).
        let lights_scene =
            |id: InstanceId| lighting.is_some_and(|l| id == l || dom.parent(id) == Some(l));
        let ids = RenderIds::get();
        let any_of =
            |id: InstanceId, classes: &[ClassId]| classes.iter().any(|&c| dom.is_a_class(id, c));
        let changes = &mut self.render_changes;
        let (first, journal) = self.render_watch.look(dom);
        rook_types::count!(JournalReadRender, journal.len());
        let adorned = &mut self.adorned;
        let material_parts = &mut self.material_parts;
        if first {
            material_parts.build(dom);
            *adorned = dom
                .descendants(dom.root())
                .into_iter()
                .filter(|&i| is_adornment(dom, i))
                .filter_map(|i| Some((i, adorned_part(dom, i)?)))
                .collect();
        }
        // A mesh decides how its parent part looks (SpecialMesh, BlockMesh,
        // CylinderMesh, FileMesh), and decals and textures draw on it.
        let decorates = |id: InstanceId| any_of(id, &ids.decorations);
        let light = |id: InstanceId| dom.is_a_class(id, ids.light);
        let effect = |id: InstanceId| is_effect_class(dom.class_name(id));
        let wire = |id: InstanceId| dom.class_name(id) == "FloorWire";
        let mark_part = |changes: &mut RenderChanges, part| mark_part(dom, changes, part);
        // An adornment draws on its Adornee: its old and new ones change,
        // and only in what it draws there. A Highlight's covering changes
        // with its Adornee, its parent and `Enabled`; its look alone
        // changes no part.
        let adorn = |changes: &mut RenderChanges,
                     adorned: &mut rustc_hash::FxHashMap<InstanceId, InstanceId>,
                     id: InstanceId,
                     gone: bool,
                     property: Option<PropertyId>| {
            let highlight = dom.is_a_class(id, ids.highlight);
            if highlight && property.is_some_and(|p| ids.highlight_look.contains(&p)) {
                changes.highlights.insert(id);
                return;
            }
            if highlight && property.is_some_and(|p| !ids.highlight_cover.contains(&p)) {
                return;
            }
            let new = if gone { None } else { adorned_part(dom, id) };
            let old = match new {
                Some(part) => adorned.insert(id, part),
                None => adorned.remove(&id),
            };
            let mark = if highlight {
                mark_highlighted
            } else {
                mark_adorned
            };
            mark(dom, changes, old);
            if new != old {
                mark(dom, changes, new);
            }
        };
        let adornment = |id: InstanceId| is_adornment(dom, id);
        for c in journal {
            match c {
                Change::Voxels { id, bounds } => changes.terrain_edits.push((*id, *bounds)),
                Change::Property { id, property } => {
                    // Part properties first: physics writes every moving
                    // part's CFrame each frame. Terrain is a BasePart too.
                    if ids.velocities.contains(property) {
                        // Nothing draws them.
                    } else if dom.is_a_class(*id, ids.viewport_frame) {
                        if ids.viewport_look.contains(property) {
                            changes.viewports.insert(*id);
                        }
                    } else if dom.is_a_class(*id, ids.camera) {
                        // The frames showing it; Workspace's camera is
                        // read every frame anyway.
                        let frames = (dom.referrers(*id).iter())
                            .filter(|&&(_, p)| p == ids.viewport_look[0])
                            .map(|&(r, _)| r);
                        changes.viewports.extend(frames);
                    } else if *property == ids.cframe && !dom.is_a_class(*id, ids.terrain) {
                        part_moved(dom, changes, *id);
                        // Physics journals only an assembly's root when it
                        // all moves.
                        for p in world.physics.moved_with(*id).filter(|p| p != id) {
                            part_moved(dom, changes, p);
                        }
                    } else if lights_scene(*id) {
                        changes.lighting = true;
                    } else if dom.is_a_class(*id, ids.terrain) || *property == ids.global_wind {
                        changes.terrain = true;
                        changes.terrain_voxels |= Some(*property) == ids.smooth_grid;
                    } else if dom.is_a_class(*id, ids.material_service) {
                        let name = rook_reflection::database().property(*property).name;
                        let overridden = name.strip_suffix("Name").and_then(Material::from_name);
                        if let Some(material) = overridden {
                            material_parts.override_changed(material, changes);
                        } else if name.starts_with("Use2022Materials") {
                            // It swaps every built-in material's textures.
                            changes.full = true;
                        }
                    } else if dom.is_a_class(*id, ids.material_variant) {
                        material_parts.note(dom, *id, changes);
                    } else if effect(*id) {
                        changes.effects.insert(*id);
                    } else if light(*id) || dom.is_a_class(*id, ids.attachment) {
                        // Attachments carry effects as well as lights.
                        changes.lights.insert(*id);
                        changes.effects.insert(*id);
                    } else if decorates(*id) {
                        mark_part(changes, dom.parent(*id));
                    } else if adornment(*id) {
                        adorn(changes, adorned, *id, false, Some(*property));
                        if wire(*id) {
                            changes.effects.insert(*id);
                        }
                    } else if dom.is_a_class(*id, ids.base_part) {
                        if ids.part_materials.contains(property) {
                            material_parts.note(dom, *id, changes);
                        }
                        mark_part(changes, Some(*id));
                    }
                    // Anything else's own properties (a Model's Name or
                    // WorldPivot, a value) draw nothing: what decorates or
                    // adorns a Model is handled above.
                }
                Change::Reparented {
                    id,
                    old_parent,
                    new_parent,
                    subtree,
                    ..
                } => {
                    for &i in subtree {
                        material_parts.note(dom, i, changes);
                    }
                    // Which Terrain Workspace holds may have changed.
                    changes.terrain |= subtree.iter().any(|&i| dom.is_a_class(i, ids.terrain));
                    // A Sky (or tone curve) arriving in or leaving Lighting.
                    changes.lighting |=
                        lighting.is_some_and(|l| *old_parent == Some(l) || *new_parent == Some(l));
                    let attachment = |i: InstanceId| dom.is_a_class(i, ids.attachment);
                    changes
                        .lights_placed
                        .extend(subtree.iter().filter(|&&i| light(i)));
                    changes.effects_placed.extend(
                        (subtree.iter()).filter(|&&i| effect(i) || wire(i) || attachment(i)),
                    );
                    for &i in subtree.iter().filter(|i| adornment(**i)) {
                        adorn(changes, adorned, i, false, None);
                    }
                    let frame = |i: InstanceId| dom.is_a_class(i, ids.viewport_frame);
                    changes
                        .viewports
                        .extend(subtree.iter().filter(|&&i| frame(i)));
                    let parts = subtree
                        .iter()
                        .filter(|&&i| dom.is_a_class(i, ids.base_part));
                    let before = changes.placed.len();
                    changes.placed.extend(parts);
                    if changes.placed.len() == before && decorates(*id) {
                        mark_part(changes, *old_parent);
                        mark_part(changes, *new_parent);
                    }
                }
                Change::Collected(id) => {
                    changes.collect(*id);
                    adorned.remove(id);
                    material_parts.forget(*id);
                }
                Change::Destroying(id) if dom.is_a_class(*id, ids.terrain) => {
                    changes.terrain = true;
                }
                Change::Destroying(id) if dom.is_a_class(*id, ids.base_part) => {
                    changes.placed.insert(*id);
                }
                Change::Destroying(id) if dom.is_a_class(*id, ids.viewport_frame) => {
                    changes.viewports.insert(*id);
                }
                Change::Destroying(id) if light(*id) => {
                    changes.lights_placed.insert(*id);
                }
                Change::Destroying(id) if effect(*id) => {
                    changes.effects_placed.insert(*id);
                }
                Change::Destroying(id) if adornment(*id) => {
                    adorn(changes, adorned, *id, true, None);
                    if wire(*id) {
                        changes.effects_placed.insert(*id);
                    }
                }
                _ => {}
            }
        }
        if changes.full {
            changes.lights.clear();
            changes.lights_placed.clear();
            changes.effects.clear();
            changes.effects_placed.clear();
            changes.parts.clear();
            changes.moved.clear();
            changes.placed.clear();
            changes.adorned.clear();
            changes.highlighted.clear();
            changes.highlights.clear();
            changes.viewports.clear();
        } else {
            // A part redrawn in full is drawn where it is now anyway, with
            // its adornments and covering.
            let (parts, placed) = (&changes.parts, &changes.placed);
            let redrawn = |id: &InstanceId| parts.contains(id) || placed.contains(id);
            changes.moved.retain(|id| !redrawn(id));
            changes.adorned.retain(|id| !redrawn(id));
            changes.highlighted.retain(|id| !redrawn(id));
            changes.parts.retain(|id| !placed.contains(id));
        }
    }
}
