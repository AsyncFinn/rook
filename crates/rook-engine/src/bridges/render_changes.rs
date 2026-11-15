mod journal;
mod materials;

use rook_reflection::{ClassId, PropertyId};
use rook_types::Material;

pub(crate) use materials::MaterialParts;

use crate::*;

/// Changes a renderer needs to apply. When `full` is set (a new
/// DataModel) the whole scene should be built; otherwise `placed` parts
/// may have entered or left the tree (parented, moved between parents or
/// destroyed), `parts` changed in any other way, `moved` only in their
/// CFrame (what physics does to every moving part every step), plus
/// Lighting if `lighting`, the terrain's look (a Terrain's properties,
/// `Workspace.GlobalWind`) if `terrain`, the local lights (and
/// Attachments, which may carry one) whose properties changed in `lights`,
/// and the lights entering or leaving the tree in `lights_placed`; the
/// same for the visual effects ([`is_effect_class`]) in `effects` and
/// `effects_placed`, plus the effect methods scripts called. A FloorWire
/// is both an adornment of its `From` part (its wire) and an effect (its
/// moving images); moving either of its parts redraws its wire. Instances
/// freed since are in `collected` alone: a renderer drops what it keeps of
/// them without reading them.
#[derive(Debug, Clone, Default)]
pub struct RenderChanges {
    pub full: bool,
    pub lighting: bool,
    pub terrain: bool,
    /// The Terrain's voxels were written whole (its `SmoothGrid`).
    pub terrain_voxels: bool,
    /// Regions of a Terrain's voxels edited, in journal order.
    pub terrain_edits: Vec<(InstanceId, rook_terrain::grid::edit::Bounds)>,
    pub lights: rustc_hash::FxHashSet<InstanceId>,
    pub lights_placed: rustc_hash::FxHashSet<InstanceId>,
    pub effects: rustc_hash::FxHashSet<InstanceId>,
    /// With the Attachments that moved in the tree, which Beams and Trails
    /// run between.
    pub effects_placed: rustc_hash::FxHashSet<InstanceId>,
    /// `ParticleEmitter:Emit` and the like, in the order called.
    pub effect_calls: Vec<(InstanceId, rook_lua::EffectCall)>,
    pub placed: rustc_hash::FxHashSet<InstanceId>,
    pub parts: rustc_hash::FxHashSet<InstanceId>,
    pub moved: rustc_hash::FxHashSet<InstanceId>,
    /// Parts whose adornments alone changed: their own look doesn't.
    pub adorned: rustc_hash::FxHashSet<InstanceId>,
    /// Parts a different Highlight may cover now.
    pub highlighted: rustc_hash::FxHashSet<InstanceId>,
    /// Highlights whose look changed.
    pub highlights: rustc_hash::FxHashSet<InstanceId>,
    /// ViewportFrames whose look may have changed: one of their own
    /// properties it's read from, a property of the Camera they show
    /// (`CurrentCamera`), or the frame entering or leaving the tree.
    pub viewports: rustc_hash::FxHashSet<InstanceId>,
    pub collected: rustc_hash::FxHashSet<InstanceId>,
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
            && !self.terrain_voxels
            && self.terrain_edits.is_empty()
            && self.lights.is_empty()
            && self.lights_placed.is_empty()
            && self.effects.is_empty()
            && self.effects_placed.is_empty()
            && self.effect_calls.is_empty()
            && self.placed.is_empty()
            && self.parts.is_empty()
            && self.moved.is_empty()
            && self.adorned.is_empty()
            && self.highlighted.is_empty()
            && self.highlights.is_empty()
            && self.viewports.is_empty()
            && self.collected.is_empty()
    }

    /// What may be somewhere new (unless `full`, which moves everything):
    /// parts moved, changed, entering or leaving the tree, or freed, and
    /// Attachments changed or moved in the tree (with the lights and
    /// effects they're among), for what follows them (world GUI).
    pub fn maybe_moved(&self) -> impl Iterator<Item = InstanceId> + '_ {
        (self.moved.iter())
            .chain(&self.parts)
            .chain(&self.placed)
            .chain(&self.collected)
            .chain(&self.lights)
            .chain(&self.effects_placed)
            .copied()
    }

    /// `id` is being freed: no other change may name it.
    fn collect(&mut self, id: InstanceId) {
        self.lights.remove(&id);
        self.lights_placed.remove(&id);
        self.effects.remove(&id);
        self.effects_placed.remove(&id);
        self.effect_calls.retain(|(e, _)| *e != id);
        self.terrain_edits.retain(|(t, _)| *t != id);
        self.placed.remove(&id);
        self.parts.remove(&id);
        self.moved.remove(&id);
        self.adorned.remove(&id);
        self.highlighted.remove(&id);
        self.highlights.remove(&id);
        self.viewports.remove(&id);
        self.collected.insert(id);
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
    (RenderIds::get().adornments.iter()).any(|&base| dom.is_a_class(id, base))
}

/// What adornment `id` is drawn with: its `Adornee`; a FloorWire's `From`;
/// a SelectionPartLasso's Humanoid's root part; a Highlight's `Adornee`,
/// else its parent (a part, or a Model whose parts it covers).
pub fn adorned_part(dom: &Dom, id: InstanceId) -> Option<InstanceId> {
    match dom.class_name(id) {
        "FloorWire" => dom.ref_property(id, "From"),
        "SelectionPartLasso" => dom
            .ref_property(id, "Humanoid")
            .and_then(|h| rook_lua::humanoid_root(dom, h)),
        "Highlight" => dom.ref_property(id, "Adornee").or_else(|| dom.parent(id)),
        _ => dom.ref_property(id, "Adornee"),
    }
}

/// `part` redraws in full.
fn mark_part(dom: &Dom, changes: &mut RenderChanges, part: Option<InstanceId>) {
    if let Some(part) =
        part.filter(|&p| !changes.full && dom.is_a_class(p, RenderIds::get().base_part))
    {
        changes.parts.insert(part);
    }
}

/// The adornments drawn on `part` changed: they alone redraw. Only a
/// Highlight draws on a Model.
fn mark_adorned(dom: &Dom, changes: &mut RenderChanges, part: Option<InstanceId>) {
    if let Some(part) =
        part.filter(|&p| !changes.full && dom.is_a_class(p, RenderIds::get().base_part))
    {
        changes.adorned.insert(part);
    }
}

/// Which Highlight covers `target` (a part, or each part of a Model) may
/// have changed: their covering alone is looked up again.
fn mark_highlighted(dom: &Dom, changes: &mut RenderChanges, target: Option<InstanceId>) {
    let Some(target) = target.filter(|_| !changes.full) else {
        return;
    };
    let c = RenderIds::get();
    if dom.is_a_class(target, c.base_part) {
        changes.highlighted.insert(target);
    } else if dom.is_a_class(target, c.model) && !dom.is_a_class(target, c.world_root) {
        let parts = dom.descendants(target).into_iter();
        changes
            .highlighted
            .extend(parts.filter(|&p| dom.is_a_class(p, c.base_part)));
    }
}

/// `id` is drawn somewhere else: it moves, and so do the FloorWires and
/// lassos drawn with it (a FloorWire's path bends as either of its parts
/// moves, and a lasso's as its Part or root part does).
fn part_moved(dom: &Dom, changes: &mut RenderChanges, id: InstanceId) {
    if !changes.full
        && !changes.parts.contains(&id)
        && dom.is_a_class(id, RenderIds::get().base_part)
    {
        changes.moved.insert(id);
    }
    for &(r, _) in dom.referrers(id) {
        if dom.class_name(r) == "FloorWire" || dom.class_name(r) == "SelectionPartLasso" {
            mark_adorned(dom, changes, adorned_part(dom, r));
        }
    }
    if let Some(h) = rook_lua::humanoid_of_root(dom, id)
        && (dom.referrers(h).iter()).any(|&(r, _)| adorned_part(dom, r) == Some(id))
    {
        mark_adorned(dom, changes, Some(id));
    }
}

impl DataModel {
    /// Where this frame draws the moving parts this side simulates
    /// (between their last two physics steps), and the current camera
    /// moved with them when it follows one: a camera script places it
    /// from where scripts see its subject, so it's moved by as much as the
    /// subject's root part is drawn away from there, which keeps the
    /// subject still on screen. Parts drawn somewhere new redraw; those
    /// drawn where they were last frame don't.
    pub(crate) fn place_drawn(&mut self) {
        let camera = self.current_camera();
        let mut w = self.runtime.world_mut();
        let w = &mut *w;
        let mut drawn: Vec<(InstanceId, rook_types::CFrame)> = w.physics.drawn_poses().collect();
        if drawn.is_empty() && w.dom.drawn_ids().next().is_none() {
            return;
        }
        let dom = &w.dom;
        let followed = camera.and_then(|c| {
            let subject = dom.ref_property(c, "CameraSubject")?;
            let root = if dom.is_a(subject, "Humanoid") {
                rook_lua::humanoid_root(dom, subject)?
            } else {
                subject
            };
            let at = drawn.iter().find(|(id, _)| *id == root)?.1.position;
            let mut cf = dom.cframe_property(c, "CFrame")?;
            cf.position += at - dom.cframe_property(root, "CFrame")?.position;
            Some((c, cf))
        });
        drawn.extend(followed);
        let moved = w.dom.set_drawn(drawn);
        let dom = &w.dom;
        let changes = &mut self.render_changes;
        for id in moved {
            part_moved(dom, changes, id);
        }
    }
}

/// The properties and classes render-change tracking checks every
/// journal entry against, resolved once.
#[derive(Debug)]
pub(crate) struct RenderIds {
    cframe: PropertyId,
    /// What a part's MaterialVariant is found by.
    part_materials: [PropertyId; 2],
    velocities: [PropertyId; 2],
    global_wind: PropertyId,
    base_part: ClassId,
    model: ClassId,
    world_root: ClassId,
    terrain: ClassId,
    smooth_grid: Option<PropertyId>,
    light: ClassId,
    attachment: ClassId,
    material_service: ClassId,
    material_variant: ClassId,
    /// What draws on its parent part: meshes, decals and textures,
    /// SurfaceAppearances.
    decorations: [ClassId; 3],
    /// See [`is_adornment`].
    adornments: [ClassId; 6],
    highlight: ClassId,
    /// What a Highlight covers by (besides where it is).
    highlight_cover: [PropertyId; 2],
    /// What a Highlight looks like.
    highlight_look: [PropertyId; 5],
    viewport_frame: ClassId,
    /// What a ViewportFrame's look is read from (`rook-scene`).
    viewport_look: [PropertyId; 6],
    camera: ClassId,
}

impl RenderIds {
    fn get() -> &'static Self {
        static IDS: std::sync::LazyLock<RenderIds> = std::sync::LazyLock::new(|| {
            let db = rook_reflection::database();
            let class = |name| db.class_by_name(name).expect(name).id;
            let part = class("BasePart");
            let id = |name| db.find_property(part, name).expect(name).id;
            RenderIds {
                cframe: id("CFrame"),
                part_materials: [id("Material"), id("MaterialVariant")],
                velocities: [id("AssemblyLinearVelocity"), id("AssemblyAngularVelocity")],
                global_wind: db
                    .find_property(class("Workspace"), "GlobalWind")
                    .expect("Workspace.GlobalWind")
                    .id,
                base_part: part,
                model: class("Model"),
                world_root: class("WorldRoot"),
                terrain: class("Terrain"),
                smooth_grid: db
                    .find_canonical_property(class("Terrain"), "SmoothGrid")
                    .map(|p| p.id),
                light: class("Light"),
                attachment: class("Attachment"),
                material_service: class("MaterialService"),
                material_variant: class("MaterialVariant"),
                decorations: ["DataModelMesh", "FaceInstance", "SurfaceAppearance"].map(class),
                adornments: [
                    "InstanceAdornment",
                    "PVAdornment",
                    "PartAdornment",
                    "FloorWire",
                    "SelectionPartLasso",
                    "Highlight",
                ]
                .map(class),
                highlight: class("Highlight"),
                highlight_cover: ["Adornee", "Enabled"]
                    .map(|name| db.find_property(class("Highlight"), name).expect(name).id),
                highlight_look: [
                    "FillColor",
                    "FillTransparency",
                    "OutlineColor",
                    "OutlineTransparency",
                    "DepthMode",
                ]
                .map(|name| db.find_property(class("Highlight"), name).expect(name).id),
                viewport_frame: class("ViewportFrame"),
                viewport_look: [
                    "CurrentCamera",
                    "Ambient",
                    "LightColor",
                    "LightDirection",
                    "BackgroundColor3",
                    "BackgroundTransparency",
                ]
                .map(|name| {
                    (db.find_property(class("ViewportFrame"), name))
                        .expect(name)
                        .id
                }),
                camera: class("Camera"),
            }
        });
        &IDS
    }
}
