//! What a DataModel draws: a render [`Scene`] built from its DOM and kept
//! up to date as scripts change it ([`SceneCache`]), the mesh files and
//! images it needs, and what the renderer takes from the Roblox install
//! ([`Installed`]). Apps that draw use it between `rook-engine` and
//! `rook-render`; a server never links it (`docs/design/architecture.md#crate-layout`).

mod camera;
mod content;
mod frame;
mod gui;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};
use rook_content::{ContentRef, MaterialMap, Resolver, Want};
use rook_dom::{Dom, InstanceId};
use rook_engine::RenderChanges;
use rook_physics::{is_part, part_shape, workspace_parts};
use rook_render::{
    Curve, DrawShape, ImageHash, ImagePlacement, Layer, Lighting, MapAlpha, MaterialMaps, MeshHash,
    PartInstance, Scene, Sequence,
};
use rook_types::{
    CFrame, Color3, Material, MaterialSet, MeshHost, MeshKind, MeshLook, MeshType, NormalId,
    SurfaceType, TilePattern, Tiling, TrussLook, TrussStyle, Value, mesh_part_scale,
};

use content::loader::{FileRef, FileState, Files, Loaded};

mod adornments;
mod effects;
mod lighting;
mod look;
mod props;
mod terrain;

pub use camera::{SavedCamera, initial_camera};
pub use content::installed::Installed;

use content::pending::*;
use content::report::Report;
use effects::{EffectCache, EffectLists, EffectTextures, Images};
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

/// What a part draws: itself, its image layers and adornments, and how
/// its CFrame places each.
struct Drawn {
    part: PartInstance,
    pose: Pose,
    layers: Vec<(PartInstance, Pose)>,
}

/// What [`SceneCache::apply`] changed in the scene.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SceneChange {
    None,
    /// These entries (indices into `scene.parts`, which may have grown),
    /// and maybe the lighting.
    Parts {
        changed: Vec<usize>,
        /// Entries that only moved: same look, new place.
        moved: Vec<usize>,
    },
    /// Everything was rebuilt.
    Full,
}

/// The render scene for a DataModel, kept between frames so that only the
/// parts scripts changed are rebuilt. Each drawn part is an entry in
/// `scene.parts`, and so is each of its Decals and Textures.
#[derive(Debug)]
pub struct SceneCache {
    pub scene: Scene,
    /// Where each drawn part is in `scene.parts`.
    index: HashMap<InstanceId, usize>,
    /// Where each drawn part's image layers and adornments are, in order.
    layers: HashMap<InstanceId, Vec<usize>>,
    /// How each entry's part's CFrame places it, by index.
    poses: Vec<Pose>,
    /// Indices of entries no longer drawn (hidden), reused for new ones so
    /// indices stay put and the renderer can update entries one by one.
    free: Vec<usize>,
    /// Mesh files and images, loaded as parts need them.
    files: Files,
    /// Parts waiting for each file, redrawn when it arrives.
    waiting: HashMap<FileRef, HashSet<InstanceId>>,
    /// Where the camera is, for loading the files nearest it first.
    focus: Vec3,
    /// Where the camera was when the waiting files were last ranked.
    ranked_from: Vec3,
    /// Where the part being looked at is (files it asks for are ranked by
    /// its distance from `focus`).
    near: Vec3,
    /// What each part's mesh file last drew.
    shown: HashMap<InstanceId, Shown>,
    /// The skybox images the lighting waits for.
    sky_files: HashSet<FileRef>,
    /// The lighting changed outside [`SceneCache::apply`] (a skybox image
    /// arrived).
    lighting_changed: bool,
    /// Workspace's Terrain, meshed.
    terrain: TerrainCache,
    /// Every local light in Workspace, and the parts they hang from
    /// (directly or through an Attachment), which move them.
    light_ids: Vec<InstanceId>,
    /// Indices into `light_ids` of the lights each part or Attachment
    /// carries, and of each light itself.
    light_holders: HashMap<InstanceId, Vec<usize>>,
    /// The visual effects, as `scene.emitters`, `beams`, `trails` and
    /// `floor_wires`.
    effects: EffectCache,
    /// An effect changed outside [`SceneCache::apply`] (its image
    /// arrived).
    effects_loaded: bool,
    /// The lines scripts gave WireframeHandleAdornments.
    wire_lines: adornments::wireframe::WireLines,
    /// Models' Highlights, found again each update.
    model_highlights: adornments::highlight::ModelHighlights,
    /// Files that didn't load, reported once loading settles.
    report: Report,
    /// The GUI layout the renderer was last given.
    gui_generation: Option<u64>,
    /// The GUI's images still loading, with the URLs it asked for each by.
    gui_waiting: HashMap<FileRef, Vec<String>>,
    /// The GUI's images that finished, for it to take.
    gui_arrived: Vec<(String, Option<rook_gui::GuiImage>)>,
}

impl SceneCache {
    /// Mesh files and images resolve through `content`, those nearest
    /// `camera` first.
    pub(crate) fn new(dom: &Dom, content: Arc<Resolver>, camera: Vec3) -> Self {
        let mut scene = Scene::default();
        let textures = EffectTextures::load(&content, &mut scene.images);
        let mut cache = Self {
            scene,
            index: HashMap::new(),
            layers: HashMap::new(),
            files: Files::new(content),
            waiting: HashMap::new(),
            focus: camera,
            ranked_from: camera,
            near: camera,
            shown: HashMap::new(),
            poses: Vec::new(),
            free: Vec::new(),
            sky_files: HashSet::new(),
            lighting_changed: false,
            terrain: TerrainCache::default(),
            light_ids: Vec::new(),
            light_holders: HashMap::new(),
            effects: EffectCache::new(textures),
            effects_loaded: false,
            wire_lines: Default::default(),
            model_highlights: Default::default(),
            report: Report::default(),
            gui_generation: None,
            gui_waiting: HashMap::new(),
            gui_arrived: Vec::new(),
        };
        cache.rebuild(dom);
        cache
    }

    /// Rebuild everything (parts added, removed or moved).
    pub(crate) fn rebuild(&mut self, dom: &Dom) {
        profiling::scope!("SceneCache::rebuild");
        self.model_highlights.clear();
        self.scene.parts.clear();
        self.poses.clear();
        self.free.clear();
        self.index.clear();
        self.layers.clear();
        for id in workspace_parts(dom) {
            if let Some(drawn) = self.part(dom, id) {
                self.index.insert(id, self.scene.parts.len());
                self.scene.parts.push(drawn.part);
                self.poses.push(drawn.pose);
                let mut layers = Vec::new();
                for (layer, pose) in drawn.layers {
                    layers.push(self.scene.parts.len());
                    self.scene.parts.push(layer);
                    self.poses.push(pose);
                }
                if !layers.is_empty() {
                    self.layers.insert(id, layers);
                }
            }
        }
        self.add_terrain(dom);
        let index = &self.index;
        self.shown.retain(|id, _| index.contains_key(id));
        self.scene.lighting = self.lighting(dom);
        self.find_lights(dom);
        self.with_effects(|effects, drawn, images| effects.find(dom, drawn, images));
    }

    /// Apply a frame's changes.
    pub(crate) fn apply(&mut self, dom: &Dom, changes: &RenderChanges) -> SceneChange {
        profiling::scope!("SceneCache::apply");
        self.model_highlights.clear();
        self.effects.note_calls(&changes.effect_calls);
        let wired =
            adornments::wireframe::note_calls(dom, &mut self.wire_lines, &changes.effect_calls);
        if changes.full {
            self.rebuild(dom);
            return SceneChange::Full;
        }
        if changes.lighting {
            self.scene.lighting = self.lighting(dom);
        }
        let lights = self.lights_changed(dom, changes);
        let effects = self
            .with_effects(|effects, drawn, images| effects.changed(dom, changes, drawn, images));
        let mut changed = Vec::new();
        let mut moved = Vec::new();
        if changes.terrain {
            match self.terrain_changed(dom) {
                Some(entries) => changed.extend(entries),
                None => {
                    self.rebuild(dom);
                    return SceneChange::Full;
                }
            }
        }
        let workspace = dom.find_service("Workspace");
        for id in &changes.placed {
            // Only parts in Workspace are drawn.
            let drawn =
                workspace.is_some_and(|ws| is_part(dom, *id) && dom.is_ancestor_of(ws, *id));
            self.place(dom, *id, drawn, &mut changed);
        }
        for id in changes.parts.iter().chain(&wired) {
            // Parts outside Workspace aren't drawn.
            if self.index.contains_key(id) {
                self.place(dom, *id, true, &mut changed);
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
            let entries =
                std::iter::once(i).chain(self.layers.get(id).into_iter().flatten().copied());
            for i in entries {
                let pose = self.poses[i];
                let part = &mut self.scene.parts[i];
                part.position = cf.position + cf.rotation * pose.offset;
                part.rotation = cf.rotation * pose.turn;
                moved.push(i);
            }
        }
        // The terrain's look (waves, wind) reaches the renderer with any
        // update, even one that changed no entry.
        if changed.is_empty()
            && moved.is_empty()
            && !changes.lighting
            && !changes.terrain
            && !lights
            && !effects
        {
            SceneChange::None
        } else {
            SceneChange::Parts { changed, moved }
        }
    }

    /// An index for a new entry.
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
        let old_layers = self.layers.remove(&id).unwrap_or_default();
        let Some(Drawn { part, pose, layers }) = look else {
            if let Some(i) = self.index.remove(&id) {
                self.shown.remove(&id);
                self.release(i);
                changed.push(i);
            }
            for i in old_layers {
                self.release(i);
                changed.push(i);
            }
            return;
        };
        match self.index.get(&id) {
            Some(&i) => {
                self.scene.parts[i] = part;
                self.poses[i] = pose;
                changed.push(i);
            }
            None => {
                let i = self.take_index(part, pose);
                self.index.insert(id, i);
                changed.push(i);
            }
        }
        // Layers keep their indices in order; extra ones are released.
        let mut kept = Vec::with_capacity(layers.len());
        let mut old = old_layers.into_iter();
        for (layer, pose) in layers {
            let i = match old.next() {
                Some(i) => {
                    self.scene.parts[i] = layer;
                    self.poses[i] = pose;
                    i
                }
                None => self.take_index(layer, pose),
            };
            kept.push(i);
            changed.push(i);
        }
        for i in old {
            self.release(i);
            changed.push(i);
        }
        if !kept.is_empty() {
            self.layers.insert(id, kept);
        }
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
        let effects =
            self.with_effects(|cache, drawn, images| cache.reread_ids(dom, &ids, drawn, images));
        effects || !changed.is_empty()
    }

    /// Whether the lighting or an effect changed outside
    /// [`SceneCache::apply`] (an image they wait for arrived).
    pub(crate) fn take_lighting_change(&mut self) -> bool {
        std::mem::take(&mut self.lighting_changed) | std::mem::take(&mut self.effects_loaded)
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
        let drawn = EffectLists {
            emitters: &mut emitters,
            beams: &mut beams,
            trails: &mut trails,
            floor_wires: &mut floor_wires,
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
        out
    }
}
