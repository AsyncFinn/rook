//! Tracking what a renderer needs to redraw.

use crate::*;

/// Changes a renderer needs to apply. When `full` is set (a new
/// DataModel) the whole scene should be built; otherwise `placed` parts
/// may have entered or left the tree (parented, moved between parents or
/// destroyed), `parts` changed in any other way, `moved` only in their
/// CFrame (what physics does to every moving part every step), plus
/// Lighting if `lighting`, the terrain's look (a Terrain's properties,
/// `Workspace.GlobalWind`) if `terrain`, the local lights (and
/// Attachments, which may carry one) whose properties changed in `lights`,
/// and lights entering or leaving the tree if `lights_placed`; the same
/// for the visual effects ([`is_effect_class`]) in `effects` and
/// `effects_placed`, plus the effect methods scripts called.
#[derive(Debug, Clone, Default)]
pub struct RenderChanges {
    pub full: bool,
    pub lighting: bool,
    pub terrain: bool,
    pub lights: std::collections::HashSet<InstanceId>,
    pub lights_placed: bool,
    /// Effects whose properties changed, and whether any entered or left
    /// the tree (`effects_placed`).
    pub effects: std::collections::HashSet<InstanceId>,
    pub effects_placed: bool,
    /// `ParticleEmitter:Emit` and the like, in the order called.
    pub effect_calls: Vec<(InstanceId, rook_lua::EffectCall)>,
    pub placed: std::collections::HashSet<InstanceId>,
    pub parts: std::collections::HashSet<InstanceId>,
    pub moved: std::collections::HashSet<InstanceId>,
}

impl RenderChanges {
    pub(crate) fn everything() -> Self {
        Self {
            full: true,
            lighting: true,
            ..Self::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.full
            && !self.lighting
            && !self.terrain
            && self.lights.is_empty()
            && !self.lights_placed
            && self.effects.is_empty()
            && !self.effects_placed
            && self.effect_calls.is_empty()
            && self.placed.is_empty()
            && self.parts.is_empty()
            && self.moved.is_empty()
    }
}

/// Whether instances of `class` are visual effects the renderer draws:
/// particles (Fire, Smoke, Sparkles, ParticleEmitter, Explosion) and
/// ribbons (Beam, Trail).
pub fn is_effect_class(class: &str) -> bool {
    matches!(
        class,
        "Fire" | "Smoke" | "Sparkles" | "ParticleEmitter" | "Beam" | "Trail" | "Explosion"
    )
}

/// The part properties render-change tracking treats specially.
#[derive(Debug)]
pub(crate) struct RenderProps {
    cframe: rook_reflection::PropertyId,
    velocities: [rook_reflection::PropertyId; 2],
    global_wind: rook_reflection::PropertyId,
}

impl RenderProps {
    fn get() -> &'static Self {
        static PROPS: std::sync::LazyLock<RenderProps> = std::sync::LazyLock::new(|| {
            let db = rook_reflection::database();
            let part = db.class_by_name("BasePart").expect("BasePart class").id;
            let id = |name| db.find_property(part, name).expect(name).id;
            RenderProps {
                cframe: id("CFrame"),
                velocities: [id("AssemblyLinearVelocity"), id("AssemblyAngularVelocity")],
                global_wind: db
                    .class_by_name("Workspace")
                    .and_then(|ws| db.find_property(ws.id, "GlobalWind"))
                    .expect("Workspace.GlobalWind")
                    .id,
            }
        });
        &PROPS
    }
}

impl DataModel {
    /// Note whether the journal since the last check touched anything drawn:
    /// parts, their meshes, decals and SurfaceAppearances, their ancestry,
    /// Lighting, the Terrain and `Workspace.GlobalWind` (which bends grass),
    /// or MaterialService and its MaterialVariants (which can change any
    /// part: the whole scene is rebuilt, as they change rarely).
    /// Camera moves don't count; the renderer reads the camera every frame
    /// anyway.
    pub(crate) fn check_render_changes(&mut self) {
        profiling::scope!("check_render_changes");
        let calls = std::mem::take(&mut self.runtime.world_mut().effect_calls);
        self.render_changes.effect_calls.extend(calls);
        let world = self.runtime.world();
        let dom = &world.dom;
        let lighting = dom.find_service("Lighting");
        let props = RenderProps::get();
        let changes = &mut self.render_changes;
        // The place's own SelectionBoxes, the first time.
        let adorned = self.adorned.get_or_insert_with(|| {
            dom.descendants(dom.root())
                .into_iter()
                .filter(|&i| dom.class_name(i) == "SelectionBox")
                .filter_map(|i| Some((i, dom.ref_property(i, "Adornee")?)))
                .collect()
        });
        // A mesh decides how its parent part looks (SpecialMesh, BlockMesh,
        // CylinderMesh, FileMesh), and decals and textures draw on it.
        let decorates = |id: InstanceId| {
            dom.is_a(id, "DataModelMesh")
                || dom.is_a(id, "FaceInstance")
                || dom.is_a(id, "SurfaceAppearance")
        };
        let light = |id: InstanceId| dom.is_a(id, "Light");
        let effect = |id: InstanceId| is_effect_class(dom.class_name(id));
        let materials =
            |id: InstanceId| dom.is_a(id, "MaterialService") || dom.is_a(id, "MaterialVariant");
        let mark_part = |changes: &mut RenderChanges, part: Option<InstanceId>| {
            if let Some(part) = part
                && !changes.full
                && dom.is_a(part, "BasePart")
            {
                changes.parts.insert(part);
            }
        };
        // A SelectionBox draws on its Adornee: its old and new ones change.
        let adorn = |changes: &mut RenderChanges,
                     adorned: &mut HashMap<InstanceId, InstanceId>,
                     id: InstanceId,
                     gone: bool| {
            let new = if gone {
                None
            } else {
                dom.ref_property(id, "Adornee")
            };
            let old = match new {
                Some(part) => adorned.insert(id, part),
                None => adorned.remove(&id),
            };
            mark_part(changes, old);
            mark_part(changes, new);
        };
        let adornment = |id: InstanceId| dom.class_name(id) == "SelectionBox";
        for c in dom.journal().since(self.render_cursor) {
            match c {
                Change::Property { id, property } => {
                    if Some(*id) == lighting {
                        changes.lighting = true;
                    } else if dom.is_a(*id, "Terrain") || *property == props.global_wind {
                        changes.terrain = true;
                    } else if materials(*id) {
                        changes.full = true;
                    } else if effect(*id) {
                        changes.effects.insert(*id);
                    } else if light(*id) || dom.is_a(*id, "Attachment") {
                        // Attachments carry effects as well as lights.
                        changes.lights.insert(*id);
                        changes.effects.insert(*id);
                    } else if props.velocities.contains(property) {
                        // Physics writes these for every moving part every
                        // step; nothing draws them.
                    } else if *property == props.cframe {
                        if !changes.full && !changes.parts.contains(id) && dom.is_a(*id, "BasePart")
                        {
                            changes.moved.insert(*id);
                        }
                    } else if decorates(*id) {
                        mark_part(changes, dom.parent(*id));
                    } else if adornment(*id) {
                        adorn(changes, adorned, *id, false);
                    } else {
                        mark_part(changes, Some(*id));
                    }
                }
                Change::Reparented {
                    id,
                    old_parent,
                    new_parent,
                    subtree,
                    ..
                } => {
                    if subtree.iter().any(|i| dom.is_a(*i, "MaterialVariant")) {
                        changes.full = true;
                    }
                    if subtree.iter().any(|i| light(*i)) {
                        changes.lights_placed = true;
                    }
                    if subtree.iter().any(|i| effect(*i)) {
                        changes.effects_placed = true;
                    }
                    for &i in subtree.iter().filter(|i| adornment(**i)) {
                        adorn(changes, adorned, i, false);
                    }
                    let parts = subtree.iter().filter(|i| dom.is_a(**i, "BasePart"));
                    let before = changes.placed.len();
                    changes.placed.extend(parts);
                    if changes.placed.len() == before && decorates(*id) {
                        mark_part(changes, *old_parent);
                        mark_part(changes, *new_parent);
                    }
                }
                Change::Destroying(id) if dom.is_a(*id, "BasePart") => {
                    changes.placed.insert(*id);
                }
                Change::Destroying(id) if light(*id) => {
                    changes.lights_placed = true;
                }
                Change::Destroying(id) if effect(*id) => {
                    changes.effects_placed = true;
                }
                Change::Destroying(id) if adornment(*id) => {
                    adorn(changes, adorned, *id, true);
                }
                _ => {}
            }
        }
        if changes.full {
            changes.lights.clear();
            changes.effects.clear();
            changes.parts.clear();
            changes.moved.clear();
            changes.placed.clear();
        } else {
            // A part redrawn in full is drawn where it is now anyway.
            let (parts, placed) = (&changes.parts, &changes.placed);
            changes
                .moved
                .retain(|id| !parts.contains(id) && !placed.contains(id));
            changes.parts.retain(|id| !placed.contains(id));
        }
        self.render_cursor = dom.journal().end();
    }
}
