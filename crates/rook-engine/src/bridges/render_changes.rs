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
/// `effects_placed`, plus the effect methods scripts called. A FloorWire
/// is both an adornment of its `From` part (its wire) and an effect (its
/// moving images); moving either of its parts redraws its wire.
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

/// Whether `id` adorns a part, drawn with it: the classes with an
/// `Adornee` (SelectionBox, SelectionSphere, the HandleAdornments,
/// Highlight, ...), SelectionPartLasso,
/// and FloorWire, drawn with its `From` part.
fn is_adornment(dom: &Dom, id: InstanceId) -> bool {
    [
        "InstanceAdornment",
        "PVAdornment",
        "PartAdornment",
        "FloorWire",
        "SelectionPartLasso",
        "Highlight",
    ]
    .iter()
    .any(|base| dom.is_a(id, base))
}

/// What adornment `id` is drawn with: its `Adornee`; a FloorWire's `From`;
/// a SelectionPartLasso's Humanoid's root part; a Highlight's `Adornee`,
/// else its parent (a part, or a Model whose parts it covers).
pub fn adorned_part(dom: &Dom, id: InstanceId) -> Option<InstanceId> {
    match dom.class_name(id) {
        "FloorWire" => dom.ref_property(id, "From"),
        "SelectionPartLasso" => dom
            .ref_property(id, "Humanoid")
            .and_then(|h| humanoid_root_part(dom, h)),
        "Highlight" => dom.ref_property(id, "Adornee").or_else(|| dom.parent(id)),
        _ => dom.ref_property(id, "Adornee"),
    }
}

/// A Humanoid's root part as a lasso starts from it (measured: the
/// HumanoidRootPart, not the Torso beside it), else its Torso
/// (Inference).
pub fn humanoid_root_part(dom: &Dom, humanoid: InstanceId) -> Option<InstanceId> {
    let model = dom.parent(humanoid)?;
    ["HumanoidRootPart", "Torso"]
        .into_iter()
        .filter_map(|name| dom.find_first_child(model, name, false))
        .find(|&p| dom.is_a(p, "BasePart"))
}

/// The Humanoid whose root part `part` is.
pub fn humanoid_of_root(dom: &Dom, part: InstanceId) -> Option<InstanceId> {
    let model = dom.parent(part)?;
    let humanoid = dom
        .children(model)
        .iter()
        .copied()
        .find(|&c| dom.class_name(c) == "Humanoid")?;
    (humanoid_root_part(dom, humanoid) == Some(part)).then_some(humanoid)
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
        let (first, journal) = self.render_watch.look(dom);
        let adorned = &mut self.adorned;
        if first {
            *adorned = dom
                .descendants(dom.root())
                .into_iter()
                .filter(|&i| is_adornment(dom, i))
                .filter_map(|i| Some((i, adorned_part(dom, i)?)))
                .collect();
        }
        // A mesh decides how its parent part looks (SpecialMesh, BlockMesh,
        // CylinderMesh, FileMesh), and decals and textures draw on it.
        let decorates = |id: InstanceId| {
            dom.is_a(id, "DataModelMesh")
                || dom.is_a(id, "FaceInstance")
                || dom.is_a(id, "SurfaceAppearance")
        };
        let light = |id: InstanceId| dom.is_a(id, "Light");
        let effect = |id: InstanceId| is_effect_class(dom.class_name(id));
        let wire = |id: InstanceId| dom.class_name(id) == "FloorWire";
        let materials =
            |id: InstanceId| dom.is_a(id, "MaterialService") || dom.is_a(id, "MaterialVariant");
        // A Model's parts all redraw (a Highlight covers them).
        let mark_part = |changes: &mut RenderChanges, part: Option<InstanceId>| {
            let Some(part) = part.filter(|_| !changes.full) else {
                return;
            };
            if dom.is_a(part, "BasePart") {
                changes.parts.insert(part);
            } else if dom.is_a(part, "Model") && !dom.is_a(part, "WorldRoot") {
                let parts = dom.descendants(part).into_iter();
                changes
                    .parts
                    .extend(parts.filter(|&p| dom.is_a(p, "BasePart")));
            }
        };
        // An adornment draws on its Adornee: its old and new ones change.
        let adorn = |changes: &mut RenderChanges,
                     adorned: &mut HashMap<InstanceId, InstanceId>,
                     id: InstanceId,
                     gone: bool| {
            let new = if gone { None } else { adorned_part(dom, id) };
            let old = match new {
                Some(part) => adorned.insert(id, part),
                None => adorned.remove(&id),
            };
            mark_part(changes, old);
            mark_part(changes, new);
        };
        let adornment = |id: InstanceId| is_adornment(dom, id);
        for c in journal {
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
                        // A FloorWire's path bends as either of its parts
                        // moves, and a lasso's as its Part does, so the
                        // part they're drawn with is drawn again.
                        for &(r, _) in dom.referrers(*id) {
                            if wire(r) || dom.class_name(r) == "SelectionPartLasso" {
                                mark_part(changes, adorned_part(dom, r));
                            }
                        }
                        if matches!(dom.name(*id), "HumanoidRootPart" | "Torso") {
                            let lassos = humanoid_of_root(dom, *id)
                                .map(|h| dom.referrers(h))
                                .unwrap_or_default();
                            if lassos
                                .iter()
                                .any(|&(r, _)| adorned_part(dom, r) == Some(*id))
                            {
                                mark_part(changes, Some(*id));
                            }
                        }
                    } else if decorates(*id) {
                        mark_part(changes, dom.parent(*id));
                    } else if adornment(*id) {
                        adorn(changes, adorned, *id, false);
                        if wire(*id) {
                            changes.effects.insert(*id);
                        }
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
                    if subtree.iter().any(|i| effect(*i) || wire(*i)) {
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
                    changes.effects_placed |= wire(*id);
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
    }
}
