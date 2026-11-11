//! What a DataModel draws: a render [`Scene`] built from its DOM and kept
//! up to date as scripts change it ([`SceneCache`]), the mesh files and
//! images it needs, and what the renderer takes from the Roblox install
//! ([`Installed`]). Apps that draw use it between `rook-engine` and
//! `rook-render`; a server never links it (`docs/design/architecture.md#crate-layout`).

mod camera;
mod content;
mod cursor;
mod frame;
mod gui;

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};
use rook_content::{ContentRef, MaterialMap, Resolver, Want};
use rook_dom::{Dom, InstanceId};
use rook_engine::RenderChanges;
use rook_physics::{is_part, part_shape, workspace_parts};
use rook_render::{
    Curve, DrawShape, Fidelity, ImageHash, ImagePlacement, Layer, Lighting, MapAlpha, MaterialMaps,
    MeshHash, PartInstance, Scene, Sequence,
};
use rook_types::{
    CFrame, Color3, Material, MaterialSet, MeshHost, MeshKind, MeshLook, MeshType, NormalId,
    SurfaceType, TilePattern, Tiling, TrussLook, TrussStyle, mesh_part_scale,
};

use content::loader::{FileRef, FileState, Files, Loaded};

mod adornments;
mod effects;
mod lighting;
mod look;
mod props;
mod terrain;

pub use camera::initial_camera;
pub use content::installed::Installed;
pub use frame::ToRender;

use adornments::floor_wire::Floors;
use content::pending::*;
use content::report::Report;
use effects::{EffectCache, EffectLists, EffectTextures, Images};
use lighting::LightCache;
use look::*;
use props::*;
use terrain::TerrainCache;

/// How a part's CFrame places what it draws: where its shape's center is
/// in the part's axes (a mesh's `Offset`, a truss's start), and the turn
/// from the shape's axes to the part's (a CylinderMesh stands upright, a
/// truss lies along its long axis).
#[derive(Debug, Clone, Copy, Default)]
struct Pose {
    offset: Vec3,
    turn: Mat3,
}

/// What a part draws: itself, its image layers and its adornments, and
/// how its CFrame places each.
struct Drawn {
    part: PartInstance,
    pose: Pose,
    images: Vec<(PartInstance, Pose)>,
    adornments: Vec<(PartInstance, Pose)>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SceneChange {
    None,
    /// These entries (indices into `scene.parts`, which may have grown),
    /// and maybe what the whole scene shares.
    Parts {
        changed: Vec<usize>,
        /// Entries that only moved: same look, new place.
        moved: Vec<usize>,
        /// The lighting or the terrain's look changed.
        lighting: bool,
        /// A Highlight's look changed ([`Scene::highlights`]).
        highlights: bool,
    },
    Full,
}

/// The render scene for a DataModel, kept between frames so that only the
/// parts scripts changed are rebuilt. Each drawn part is an entry in
/// `scene.parts`, and so is each of its Decals and Textures.
#[derive(Debug)]
pub struct SceneCache {
    pub scene: Scene,
    /// Where each drawn part is in `scene.parts`.
    index: FxHashMap<InstanceId, usize>,
    /// Where each drawn part's image layers are, in order.
    layers: FxHashMap<InstanceId, Vec<usize>>,
    /// Where each drawn part's adornments are, in order: redrawn on their
    /// own when only they change.
    adornment_layers: FxHashMap<InstanceId, Vec<usize>>,
    /// How each entry's part's CFrame places it, by index.
    poses: Vec<Pose>,
    /// Indices of entries no longer drawn (hidden), reused for new ones so
    /// indices stay put and the renderer can update entries one by one.
    free: Vec<usize>,
    files: Files,
    /// Parts waiting for each file, redrawn when it arrives.
    waiting: FxHashMap<FileRef, FxHashSet<InstanceId>>,
    /// Parts whose files arrived, drawn again a frame's budget at a time.
    arrivals: content::arrivals::Arrivals,
    /// Where the camera is, for loading the files nearest it first.
    focus: Vec3,
    /// Where the camera was when the waiting files were last ranked.
    ranked_from: Vec3,
    /// Where the part being looked at is (files it asks for are ranked by
    /// its distance from `focus`).
    near: Vec3,
    /// What each part's mesh file last drew.
    shown: FxHashMap<InstanceId, Shown>,
    /// The skybox images the lighting waits for.
    sky_files: FxHashSet<FileRef>,
    /// The lighting changed outside [`SceneCache::apply`] (a skybox image
    /// arrived).
    lighting_changed: bool,
    terrain: TerrainCache,
    lights: LightCache,
    effects: EffectCache,
    wire_lines: adornments::wireframe::WireLines,
    floors: Floors,
    /// Models' Highlights, found again each update.
    model_highlights: adornments::highlight::ModelHighlights,
    /// `scene.highlights` changed since the renderer last took it.
    highlights_changed: bool,
    report: Report,
    /// The GUI layout the renderer was last given.
    gui_generation: Option<u64>,
    /// The GUI's images still loading, with the URLs it asked for each by.
    gui_waiting: FxHashMap<FileRef, Vec<String>>,
    /// The GUI's images that finished, for it to take.
    gui_arrived: Vec<(String, Option<rook_gui::GuiImage>)>,
    hand: Option<(Arc<rook_content::decode::image::Image>, ImageHash)>,
}

impl SceneCache {
    /// Mesh files and images resolve through `content`, those nearest
    /// `camera` first.
    /// FloorWires' floors are `floors` ([`Floors::build`]).
    pub(crate) fn new(dom: &Dom, floors: Floors, content: Arc<Resolver>, camera: Vec3) -> Self {
        let mut scene = Scene::default();
        let textures = EffectTextures::load(&content, &mut scene.images);
        let mut cache = Self {
            scene,
            index: FxHashMap::default(),
            layers: FxHashMap::default(),
            adornment_layers: FxHashMap::default(),
            files: Files::new(content),
            waiting: FxHashMap::default(),
            arrivals: Default::default(),
            focus: camera,
            ranked_from: camera,
            near: camera,
            shown: FxHashMap::default(),
            poses: Vec::new(),
            free: Vec::new(),
            sky_files: FxHashSet::default(),
            lighting_changed: false,
            terrain: TerrainCache::default(),
            lights: LightCache::default(),
            effects: EffectCache::new(textures),
            wire_lines: Default::default(),
            floors,
            model_highlights: Default::default(),
            highlights_changed: false,
            report: Report::default(),
            gui_generation: None,
            gui_waiting: FxHashMap::default(),
            gui_arrived: Vec::new(),
            hand: None,
        };
        cache.rebuild(dom);
        cache
    }

    /// Rebuild everything (parts added, removed or moved).
    pub(crate) fn rebuild(&mut self, dom: &Dom) {
        profiling::scope!("SceneCache::rebuild");
        self.model_highlights.clear();
        self.scene.highlights.clear();
        self.scene.parts.clear();
        self.poses.clear();
        self.free.clear();
        self.index.clear();
        self.layers.clear();
        self.adornment_layers.clear();
        for id in workspace_parts(dom) {
            if let Some(drawn) = self.part(dom, id) {
                let base = self.take_index(drawn.part, drawn.pose);
                self.index.insert(id, base);
                let images = self.set_layers(base, Vec::new(), drawn.images, None);
                let adornments = self.set_layers(base, Vec::new(), drawn.adornments, None);
                if !images.is_empty() {
                    self.layers.insert(id, images);
                }
                if !adornments.is_empty() {
                    self.adornment_layers.insert(id, adornments);
                }
            }
        }
        self.add_terrain(dom);
        let index = &self.index;
        self.shown.retain(|id, _| index.contains_key(id));
        self.scene.lighting = self.lighting(dom);
        self.lights.build(dom, &mut self.scene.lights);
        self.with_effects(|effects, drawn, images| effects.build(dom, drawn, images));
    }

    pub(crate) fn apply(&mut self, dom: &Dom, changes: &RenderChanges) -> SceneChange {
        profiling::scope!("SceneCache::apply");
        self.model_highlights.clear();
        self.effects.note_calls(&changes.effect_calls);
        let wired =
            adornments::wireframe::note_calls(dom, &mut self.wire_lines, &changes.effect_calls);
        let mut changed = Vec::new();
        // Freed: nothing of them may be read, only dropped.
        for &id in &changes.collected {
            self.place(dom, id, false, &mut changed);
            self.wire_lines.remove(&id);
            self.highlights_changed |= self.scene.highlights.remove(&id.to_ref().0).is_some();
        }
        if !changes.collected.is_empty() {
            for parts in self.waiting.values_mut() {
                parts.retain(|p| !changes.collected.contains(p));
            }
        }
        self.note_voxels(changes);
        if changes.full {
            self.rebuild(dom);
            return SceneChange::Full;
        }
        if changes.lighting {
            self.scene.lighting = self.lighting(dom);
        }
        self.lights.apply(dom, changes, &mut self.scene.lights);
        self.with_effects(|effects, drawn, images| effects.changed(dom, changes, drawn, images));
        let mut moved = Vec::new();
        if changes.terrain || !changes.terrain_edits.is_empty() {
            changed.extend(self.terrain_changed(dom, changes.terrain));
        }
        let workspace = dom.find_service("Workspace");
        for id in &changes.placed {
            // Only parts in Workspace are drawn.
            let drawn =
                workspace.is_some_and(|ws| is_part(dom, *id) && dom.is_ancestor_of(ws, *id));
            self.place(dom, *id, drawn, &mut changed);
        }
        for id in changes.parts.iter().chain(&wired) {
            if self.index.contains_key(id) {
                self.place(dom, *id, true, &mut changed);
            }
        }
        // Adornments and Highlights alone: the parts' own looks stay.
        for id in &changes.adorned {
            if let Some(&base) = self.index.get(id) {
                let old = self.adornment_layers.remove(id).unwrap_or_default();
                let new = self.adornments(dom, *id);
                let kept = self.set_layers(base, old, new, Some(&mut changed));
                if !kept.is_empty() {
                    self.adornment_layers.insert(*id, kept);
                }
            }
        }
        for id in &changes.highlighted {
            if let Some(&i) = self.index.get(id) {
                let covering = self.highlight(dom, *id);
                if self.scene.parts[i].highlight != covering {
                    self.scene.parts[i].highlight = covering;
                    changed.push(i);
                }
            }
        }
        for &id in &changes.highlights {
            let key = id.to_ref().0;
            if self.scene.highlights.contains_key(&key) {
                let look = adornments::highlight::look(dom, id);
                self.scene.highlights.insert(key, look);
                self.highlights_changed = true;
            }
        }
        // Parts that only moved keep their look: place it at the new CFrame.
        for id in &changes.moved {
            let Some(&i) = self.index.get(id) else {
                continue;
            };
            let Some(cf) = dom.drawn_cframe(*id) else {
                continue;
            };
            let layers = [&self.layers, &self.adornment_layers].map(|l| l.get(id));
            let entries = std::iter::once(i).chain(layers.into_iter().flatten().flatten().copied());
            for i in entries {
                let pose = self.poses[i];
                let part = &mut self.scene.parts[i];
                part.position = cf.position + cf.rotation * pose.offset;
                part.rotation = cf.rotation * pose.turn;
                moved.push(i);
            }
        }
        let lighting = changes.lighting || changes.terrain;
        let highlights = std::mem::take(&mut self.highlights_changed);
        if changed.is_empty() && moved.is_empty() && !lighting && !highlights {
            SceneChange::None
        } else {
            SceneChange::Parts {
                changed,
                moved,
                lighting,
                highlights,
            }
        }
    }

    fn take_index(&mut self, entry: PartInstance, pose: Pose) -> usize {
        match self.free.pop() {
            Some(i) => {
                self.scene.parts[i] = entry;
                self.poses[i] = pose;
                i
            }
            None => {
                self.scene.parts.push(entry);
                self.poses.push(pose);
                self.scene.parts.len() - 1
            }
        }
    }

    /// Stop drawing entry `i`, keeping its index for reuse.
    fn release(&mut self, i: usize) {
        self.scene.parts[i].transparency = 1.0;
        self.free.push(i);
    }

    /// Draw `id` (when `drawn`) or stop drawing it, adding the entries that
    /// changed to `changed`.
    fn place(&mut self, dom: &Dom, id: InstanceId, drawn: bool, changed: &mut Vec<usize>) {
        let look = if drawn { self.part(dom, id) } else { None };
        let old_images = self.layers.remove(&id).unwrap_or_default();
        let old_adornments = self.adornment_layers.remove(&id).unwrap_or_default();
        let Some(Drawn {
            part,
            pose,
            images,
            adornments,
        }) = look
        else {
            if let Some(i) = self.index.remove(&id) {
                self.shown.remove(&id);
                self.release(i);
                changed.push(i);
            }
            for i in old_images.into_iter().chain(old_adornments) {
                self.release(i);
                changed.push(i);
            }
            return;
        };
        let base = match self.index.get(&id) {
            Some(&i) => {
                self.scene.parts[i] = part;
                self.poses[i] = pose;
                i
            }
            None => {
                let i = self.take_index(part, pose);
                self.index.insert(id, i);
                i
            }
        };
        changed.push(base);
        let images = self.set_layers(base, old_images, images, Some(changed));
        let adornments = self.set_layers(base, old_adornments, adornments, Some(changed));
        if !images.is_empty() {
            self.layers.insert(id, images);
        }
        if !adornments.is_empty() {
            self.adornment_layers.insert(id, adornments);
        }
    }

    /// Draw `layers` over entry `base` at `old`'s indices, in order (extra
    /// ones take free indices, leftover ones are released), adding the
    /// entries that changed to `changed`; the indices they're at now.
    fn set_layers(
        &mut self,
        base: usize,
        old: Vec<usize>,
        layers: Vec<(PartInstance, Pose)>,
        mut changed: Option<&mut Vec<usize>>,
    ) -> Vec<usize> {
        let mut kept = Vec::with_capacity(layers.len());
        let mut old = old.into_iter();
        for (layer, pose) in layers {
            let layer = PartInstance {
                drawn_over: Some(base),
                ..layer
            };
            let i = match old.next() {
                Some(i) => {
                    self.scene.parts[i] = layer;
                    self.poses[i] = pose;
                    i
                }
                None => self.take_index(layer, pose),
            };
            kept.push(i);
            if let Some(c) = changed.as_deref_mut() {
                c.push(i);
            }
        }
        for i in old {
            self.release(i);
            if let Some(c) = changed.as_deref_mut() {
                c.push(i);
            }
        }
        kept
    }

    /// Apply scripts' effect calls (`Emit` and the like) made before this
    /// scene was built from the DataModel. Whether any effect or
    /// wireframe changed.
    pub(crate) fn take_effect_calls(
        &mut self,
        dom: &Dom,
        calls: &[(InstanceId, rook_lua::EffectCall)],
    ) -> bool {
        self.effects.note_calls(calls);
        let wired = adornments::wireframe::note_calls(dom, &mut self.wire_lines, calls);
        let mut changed = Vec::new();
        for id in wired {
            if self.index.contains_key(&id) {
                self.place(dom, id, true, &mut changed);
            }
        }
        let ids: Vec<InstanceId> = calls.iter().map(|c| c.0).collect();
        self.with_effects(|cache, drawn, images| cache.reread_ids(dom, &ids, drawn, images));
        !changed.is_empty()
    }

    pub(crate) fn take_lighting_change(&mut self) -> bool {
        std::mem::take(&mut self.lighting_changed)
    }

    /// Run `f` on the effect cache with the scene's effect lists and a way
    /// to load the effects' images.
    fn with_effects<R>(
        &mut self,
        f: impl FnOnce(&mut EffectCache, EffectLists<'_>, &mut Images<'_>) -> R,
    ) -> R {
        let mut cache = std::mem::take(&mut self.effects);
        let mut emitters = std::mem::take(&mut self.scene.emitters);
        let mut beams = std::mem::take(&mut self.scene.beams);
        let mut trails = std::mem::take(&mut self.scene.trails);
        let mut floor_wires = std::mem::take(&mut self.scene.floor_wires);
        let floors = std::mem::take(&mut self.floors);
        let drawn = EffectLists {
            emitters: &mut emitters,
            beams: &mut beams,
            trails: &mut trails,
            floor_wires: &mut floor_wires,
            floors: &floors,
        };
        let mut images = |id: InstanceId, url: &str| match self.file_state(id, url, Want::Image) {
            FileState::Loaded(hash) => Some(ImageHash(hash)),
            FileState::Loading | FileState::Failed => None,
        };
        let out = f(&mut cache, drawn, &mut images);
        self.effects = cache;
        self.scene.emitters = emitters;
        self.scene.beams = beams;
        self.scene.trails = trails;
        self.scene.floor_wires = floor_wires;
        self.floors = floors;
        out
    }
}
