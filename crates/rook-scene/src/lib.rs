//! What a DataModel draws: a render [`Scene`] built from its DOM and kept
//! up to date as scripts change it ([`SceneCache`]), the mesh files and
//! images it needs, and what the renderer takes from the Roblox install
//! ([`Installed`]). Apps that draw use it between `rook-engine` and
//! `rook-render`; a server never links it (`docs/design/architecture.md#crate-layout`).

mod camera;
mod changes;
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
    MeshHash, PartInstance, Scene, Sequence, Space,
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
mod placing;
mod props;
mod terrain;
mod viewports;

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
    /// The ViewportFrames whose looks changed since the renderer last
    /// took them (`scene.viewports`' keys).
    viewports_changed: Vec<u64>,
    report: Report,
    /// The GUI layout the renderer was last given.
    gui_generation: Option<u64>,
    /// The GUI's images still loading, with the URLs it asked for each by.
    gui_waiting: FxHashMap<FileRef, Vec<String>>,
    /// The GUI's images that finished, for it to take.
    gui_arrived: Vec<(String, Option<rook_gui::GuiImage>)>,
    /// The images loaded, and those the renderer is still to take.
    images: content::images::LoadedImages,
    /// The ClickDetector hand's image once made, and its size in pixels.
    hand: Option<(ImageHash, Vec2)>,
    /// The camera and view size textures are sized on screen by
    /// ([`SceneCache::set_view`]).
    view: Option<content::streaming::View>,
    /// Counts frames, for looking over textures every few.
    stream_tick: u32,
}

impl SceneCache {
    /// Mesh files and images resolve through `content`, those nearest
    /// `camera` first.
    /// FloorWires' floors are `floors` ([`Floors::build`]).
    pub(crate) fn new(dom: &Dom, floors: Floors, content: Arc<Resolver>, camera: Vec3) -> Self {
        let mut images = content::images::LoadedImages::default();
        let textures = EffectTextures::load(&content, &mut images);
        let mut cache = Self {
            scene: Scene::default(),
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
            viewports_changed: Vec::new(),
            report: Report::default(),
            gui_generation: None,
            gui_waiting: FxHashMap::default(),
            gui_arrived: Vec::new(),
            images,
            hand: None,
            view: None,
            stream_tick: 0,
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
        // Workspace's parts, then each ViewportFrame's (those of a frame
        // inside another frame are found from both).
        let frames = self.build_viewports(dom);
        let mut parts = workspace_parts(dom);
        for frame in frames {
            parts.extend(
                dom.descendants(frame)
                    .into_iter()
                    .filter(|&p| is_part(dom, p)),
            );
        }
        for id in parts {
            if self.index.contains_key(&id) {
                continue;
            }
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
}
