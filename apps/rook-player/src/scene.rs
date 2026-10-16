//! Build a render [`Scene`] from a DataModel.

pub(crate) mod files;
pub(crate) mod installed;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};
use rook_content::{ContentRef, MaterialMap, Resolver, Want};
use rook_dom::{Dom, InstanceId};
use rook_engine::{DataModel, RenderChanges};
use rook_physics::{is_part, part_shape, workspace_parts};
use rook_render::{
    Camera, DrawShape, ImageHash, ImagePlacement, Layer, LightShape, Lighting, LocalLight,
    MapAlpha, Material, MaterialMaps, MaterialSet, MeshHash, NormalId, PartInstance, Scene, Tiling,
};
use rook_types::{
    CFrame, MeshKind, MeshLook, SurfaceType, TrussLook, TrussStyle, Value, mesh_part_scale,
};

use crate::scene::files::{FileRef, FileState, Files, Loaded};

mod lighting;
mod lights;
mod loading;
mod look;
mod materials;
mod props;
mod terrain;
mod union;

use loading::*;
use look::*;
use props::*;
use terrain::TerrainCache;
use union::*;

/// How a part's CFrame places what it draws: where its shape's centre is
/// in the part's axes (a mesh's `Offset`, a truss's start), and the turn
/// from the shape's axes to the part's (a CylinderMesh stands upright, a
/// truss lies along its long axis).
#[derive(Debug, Clone, Copy, Default)]
struct Pose {
    offset: Vec3,
    turn: Mat3,
}

/// What a part draws: itself and its image layers, and how its CFrame
/// places them.
struct Drawn {
    part: PartInstance,
    pose: Pose,
    layers: Vec<PartInstance>,
}

/// What [`SceneCache::apply`] changed in the scene.
#[derive(Debug, PartialEq, Eq)]
pub enum SceneChange {
    None,
    /// These entries (indices into `scene.parts`, which may have grown),
    /// and maybe the lighting.
    Parts(Vec<usize>),
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
    /// Where each drawn part's image layers are, in child order.
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
}

impl SceneCache {
    /// Mesh files and images resolve through `content`.
    pub fn new(dom: &Dom, content: Arc<Resolver>) -> Self {
        let mut cache = Self {
            scene: Scene::default(),
            index: HashMap::new(),
            layers: HashMap::new(),
            files: Files::new(content),
            waiting: HashMap::new(),
            shown: HashMap::new(),
            poses: Vec::new(),
            free: Vec::new(),
            sky_files: HashSet::new(),
            lighting_changed: false,
            terrain: TerrainCache::default(),
            light_ids: Vec::new(),
            light_holders: HashMap::new(),
        };
        cache.rebuild(dom);
        cache
    }

    /// Rebuild everything (parts added, removed or moved).
    pub fn rebuild(&mut self, dom: &Dom) {
        profiling::scope!("SceneCache::rebuild");
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
                for layer in drawn.layers {
                    layers.push(self.scene.parts.len());
                    self.scene.parts.push(layer);
                    self.poses.push(drawn.pose);
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
    }

    /// Find every local light in Workspace again, and read them.
    fn find_lights(&mut self, dom: &Dom) {
        profiling::scope!("SceneCache::find_lights");
        self.light_ids = lights::find_lights(dom);
        self.light_holders.clear();
        for (i, &id) in self.light_ids.iter().enumerate() {
            // A light changes with its own properties, and under an
            // Attachment moves with the attachment and its part.
            self.light_holders.entry(id).or_default().push(i);
            let mut holder = dom.parent(id);
            while let Some(h) = holder {
                self.light_holders.entry(h).or_default().push(i);
                if dom.class_name(h) != "Attachment" {
                    break;
                }
                holder = dom.parent(h);
            }
        }
        self.scene.lights = (0..self.light_ids.len())
            .map(|i| self.read_light(dom, i))
            .collect();
    }

    /// The `i`th known light as it is now; a light Studio wouldn't draw
    /// (not under a part or an Attachment) shines with brightness 0.
    fn read_light(&self, dom: &Dom, i: usize) -> LocalLight {
        lights::light(dom, self.light_ids[i]).unwrap_or(LocalLight {
            position: Vec3::ZERO,
            shape: LightShape::Point,
            color: [0.0; 3],
            brightness: 0.0,
            range: 0.0,
            shadows: false,
        })
    }

    /// Bring the lights up to date with a frame's changes, reading again
    /// only the lights that changed and those on parts and Attachments
    /// that did; whether any may have changed.
    fn lights_changed(&mut self, dom: &Dom, changes: &RenderChanges) -> bool {
        let holders = &self.light_holders;
        let placed = changes.placed.iter().any(|id| holders.contains_key(id));
        if changes.lights_placed || placed {
            self.find_lights(dom);
            return true;
        }
        let stale: Vec<usize> = changes
            .moved
            .iter()
            .chain(&changes.parts)
            .chain(&changes.lights)
            .filter_map(|id| holders.get(id))
            .flatten()
            .copied()
            .collect();
        if stale.is_empty() {
            return false;
        }
        profiling::scope!("SceneCache::read_lights");
        for i in stale {
            self.scene.lights[i] = self.read_light(dom, i);
        }
        true
    }

    /// Apply a frame's changes.
    pub fn apply(&mut self, dom: &Dom, changes: &RenderChanges) -> SceneChange {
        profiling::scope!("SceneCache::apply");
        if changes.full {
            self.rebuild(dom);
            return SceneChange::Full;
        }
        if changes.lighting {
            self.scene.lighting = self.lighting(dom);
        }
        let lights = self.lights_changed(dom, changes);
        let mut changed = Vec::new();
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
        for id in &changes.parts {
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
            let Some(Value::CFrame(cf)) = dom.property_by_name(*id, "CFrame") else {
                continue;
            };
            let entries =
                std::iter::once(i).chain(self.layers.get(id).into_iter().flatten().copied());
            for i in entries {
                let pose = self.poses[i];
                let part = &mut self.scene.parts[i];
                part.position = cf.position + cf.rotation * pose.offset;
                part.rotation = cf.rotation * pose.turn;
                changed.push(i);
            }
        }
        // The terrain's look (waves, wind) reaches the renderer with any
        // update, even one that changed no entry.
        if changed.is_empty() && !changes.lighting && !changes.terrain && !lights {
            SceneChange::None
        } else {
            SceneChange::Parts(changed)
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
        for layer in layers {
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

    /// Whether the lighting changed since the last call outside
    /// [`SceneCache::apply`] (a skybox image arrived).
    pub fn take_lighting_change(&mut self) -> bool {
        std::mem::take(&mut self.lighting_changed)
    }
}

/// The camera the place was saved with, or a view of the origin.
pub fn initial_camera(dm: &DataModel) -> Camera {
    let world = dm.runtime().world();
    let dom = &world.dom;
    let saved = dm.current_camera().and_then(|cam| {
        let Value::CFrame(cf) = dom.property_by_name(cam, "CFrame")? else {
            return None;
        };
        Some(Camera {
            position: cf.position,
            rotation: Mat3::from_cols(
                cf.rotation.x_axis.normalize(),
                cf.rotation.y_axis.normalize(),
                cf.rotation.z_axis.normalize(),
            ),
            fov_y: number(dom, cam, "FieldOfView").unwrap_or(70.0),
        })
    });
    saved.unwrap_or_else(|| Camera::look_at(Vec3::new(0.0, 20.0, 40.0), Vec3::ZERO))
}
