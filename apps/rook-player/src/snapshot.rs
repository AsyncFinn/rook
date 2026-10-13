//! Build a render [`Scene`] from a DataModel.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};
use rook_content::{ContentRef, Resolver, Want};
use rook_dom::{Dom, InstanceId};
use rook_engine::{DataModel, RenderChanges};
use rook_physics::{is_part, part_shape, workspace_parts};
use rook_render::{
    Camera, DrawShape, ImageHash, ImagePlacement, Layer, Lighting, MeshHash, NormalId,
    PartInstance, Scene, Tiling,
};
use rook_types::{Color3, MeshKind, MeshLook, Value, mesh_part_scale};

use crate::files::{FileRef, FileState, Files, Loaded};

fn number(dom: &Dom, id: InstanceId, name: &str) -> Option<f32> {
    dom.property_by_name(id, name)?
        .as_number()
        .map(|n| n as f32)
}

fn color(dom: &Dom, id: InstanceId, name: &str) -> Option<[f32; 3]> {
    let c: Color3 = match dom.property_by_name(id, name)? {
        Value::Color3(c) => c,
        Value::Color3uint8(c) => c.into(),
        _ => return None,
    };
    Some([c.r, c.g, c.b])
}

fn boolean(dom: &Dom, id: InstanceId, name: &str) -> Option<bool> {
    match dom.property_by_name(id, name)? {
        Value::Bool(b) => Some(b),
        _ => None,
    }
}

fn vector3(dom: &Dom, id: InstanceId, name: &str) -> Option<Vec3> {
    match dom.property_by_name(id, name)? {
        Value::Vector3(v) => Some(v.0),
        _ => None,
    }
}

fn enum_value(dom: &Dom, id: InstanceId, name: &str) -> Option<u32> {
    match dom.property_by_name(id, name)? {
        Value::Enum(n) => Some(n),
        _ => None,
    }
}

/// The first non-empty content URL among `names` (files store the
/// `Content` property; some old files saved the legacy name).
fn content_url(dom: &Dom, id: InstanceId, names: &[&str]) -> String {
    names
        .iter()
        .find_map(|name| {
            dom.property_by_name(id, name)?
                .content_url()
                .filter(|u| !u.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

/// The mesh child that decides how a part looks: of several, the one added
/// last (observed: a part with a SpecialMesh Sphere and a CylinderMesh drew
/// whichever was parented second). With its kind and `MeshId`.
fn part_mesh(dom: &Dom, id: InstanceId) -> Option<(MeshKind, InstanceId, String)> {
    dom.children(id).iter().rev().find_map(|&child| {
        let mesh_type = enum_value(dom, child, "MeshType");
        let mesh_id = mesh_id(dom, child);
        let kind = MeshKind::of(dom.class_name(child), mesh_type, &mesh_id)?;
        Some((kind, child, mesh_id))
    })
}

/// A mesh's or MeshPart's `MeshId`. Files store it in `MeshContent` (`MeshId`
/// is a view of it); some 2016 MeshParts saved it as `MeshID`.
fn mesh_id(dom: &Dom, id: InstanceId) -> String {
    content_url(dom, id, &["MeshContent", "MeshID"])
}

/// A mesh's `TextureId` or a MeshPart's `TextureID`.
fn texture_id(dom: &Dom, id: InstanceId) -> String {
    content_url(dom, id, &["TextureContent", "TextureID"])
}

/// Turns a shape's X axis onto Y, for a CylinderMesh.
const UPRIGHT: Mat3 = Mat3::from_cols(Vec3::Y, Vec3::NEG_X, Vec3::Z);

/// What a part's mesh file last settled on.
#[derive(Debug, Clone, Copy)]
enum Settled {
    /// A loaded mesh.
    File(MeshHash),
    /// The look of an empty `MeshId`.
    Empty,
}

/// What a part's mesh last drew, and which mesh instance drew it (the part
/// itself for a MeshPart).
#[derive(Debug, Clone, Copy)]
struct Shown {
    mesh: InstanceId,
    settled: Settled,
}

/// How a mesh file draws right now.
enum FileDraw {
    /// Its mesh, or the last one this mesh loaded.
    Mesh(MeshHash),
    /// The look of an empty `MeshId`, which this mesh had last.
    Empty,
    Nothing,
}

/// How Decals and Textures go on what a part draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decals {
    /// The part draws nothing, so neither do they.
    None,
    /// Projected onto the faces of the drawn shape's box.
    Faces,
    /// Through the mesh's own UVs (`MeshKind::decals_follow_uvs`).
    MeshUv,
}

/// Direction to the sun for a clock time and latitude. Roblox turns the sun
/// around the Z axis once a day, rising at +X, on a circle tilted towards
/// +Z by the latitude less 23.5° (Confirmed: `Lighting:GetSunDirection()`
/// in Studio 0.742 for latitudes 0, 41.733 and -30, 2026-10-13).
pub fn sun_direction(hours: f32, latitude_degrees: f32) -> Vec3 {
    let h = (hours - 12.0) / 24.0 * std::f32::consts::TAU;
    let tilt = (latitude_degrees - 23.5).to_radians();
    Vec3::new(-h.sin() * tilt.cos(), h.cos() * tilt.cos(), tilt.sin())
}

fn lighting(dom: &Dom) -> Lighting {
    let mut l = Lighting::default();
    let Some(id) = dom.find_service("Lighting") else {
        return l;
    };
    let hours = dom.clock_time(id) as f32;
    let latitude = number(dom, id, "GeographicLatitude").unwrap_or(41.733);
    l.sun_direction = sun_direction(hours, latitude);
    if let Some(c) = color(dom, id, "Ambient") {
        l.ambient = c;
    }
    if let Some(c) = color(dom, id, "OutdoorAmbient") {
        l.outdoor_ambient = c;
    }
    if let Some(b) = number(dom, id, "Brightness") {
        l.brightness = b;
    }
    if let Some(c) = color(dom, id, "FogColor") {
        l.fog_color = c;
    }
    if let Some(s) = number(dom, id, "FogStart") {
        l.fog_start = s;
    }
    if let Some(e) = number(dom, id, "FogEnd") {
        l.fog_end = e;
    }
    // Off draws no shadows (Confirmed: creator-docs Lighting.GlobalShadows;
    // defaults to on).
    if let Some(s) = boolean(dom, id, "GlobalShadows") {
        l.global_shadows = s;
    }
    // A night sky darkens towards black as the sun sets (Approximation
    // until skyboxes are supported).
    let day = (l.sun_direction.y * 4.0 + 0.5).clamp(0.05, 1.0);
    l.sky_top = l.sky_top.map(|c| c * day);
    l.sky_horizon = l.sky_horizon.map(|c| c * day);
    l
}

/// How a part's CFrame places what it draws: its mesh's `Offset` in the
/// part's axes, and whether the shape is turned upright (a CylinderMesh).
#[derive(Debug, Clone, Copy, Default)]
struct Pose {
    offset: Vec3,
    upright: bool,
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
        let index = &self.index;
        self.shown.retain(|id, _| index.contains_key(id));
        self.scene.lighting = lighting(dom);
    }

    /// Apply a frame's changes.
    pub fn apply(&mut self, dom: &Dom, changes: &RenderChanges) -> SceneChange {
        profiling::scope!("SceneCache::apply");
        if changes.full {
            self.rebuild(dom);
            return SceneChange::Full;
        }
        if changes.lighting {
            self.scene.lighting = lighting(dom);
        }
        let mut changed = Vec::new();
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
                part.rotation = if pose.upright {
                    cf.rotation * UPRIGHT
                } else {
                    cf.rotation
                };
                changed.push(i);
            }
        }
        if changed.is_empty() && !changes.lighting {
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

    /// Take the files that finished loading and redraw the parts waiting
    /// for them; with `wait`, first wait until none is loading. Returns the
    /// entries that changed.
    pub fn poll_files(&mut self, dom: &Dom, wait: bool) -> Vec<usize> {
        let finished = self.files.poll(wait);
        if finished.is_empty() {
            return Vec::new();
        }
        profiling::scope!("SceneCache::poll_files");
        let mut parts = HashSet::new();
        for (key, loaded) in finished {
            match loaded {
                Some(Loaded::Mesh(hash, mesh)) => {
                    self.scene.meshes.insert(hash, mesh);
                }
                Some(Loaded::Image(hash, image)) => {
                    self.scene.images.insert(hash, image);
                }
                None => {}
            }
            parts.extend(self.waiting.remove(&key).unwrap_or_default());
        }
        let mut changed = Vec::new();
        for id in parts {
            if self.index.contains_key(&id) {
                self.place(dom, id, true, &mut changed);
            }
        }
        changed
    }

    /// A file's state, noting that `part` waits for it while it loads.
    fn file_state(&mut self, part: InstanceId, url: &str, want: Want) -> FileState {
        let Some(r) = ContentRef::parse(url) else {
            return FileState::Failed;
        };
        let state = self.files.state(&r, want);
        if state == FileState::Loading {
            self.waiting.entry((r, want)).or_default().insert(part);
        }
        state
    }

    /// What a part draws (`None` without a CFrame or Size): itself, its
    /// Decals and Textures, and how its CFrame places them. A mesh that
    /// draws nothing leaves the part fully transparent, so it stays in the
    /// scene for a script to change back.
    fn part(&mut self, dom: &Dom, id: InstanceId) -> Option<Drawn> {
        let mut pose = Pose::default();
        let (part, decals) = self.look(dom, id, &mut pose)?;
        let layers = match decals {
            Decals::None => Vec::new(),
            Decals::Faces | Decals::MeshUv => self.images(dom, id, &part, decals),
        };
        Some(Drawn { part, pose, layers })
    }

    /// [`SceneCache::part`]'s look, filling in `pose` as it goes.
    fn look(
        &mut self,
        dom: &Dom,
        id: InstanceId,
        pose: &mut Pose,
    ) -> Option<(PartInstance, Decals)> {
        let Value::CFrame(cf) = dom.property_by_name(id, "CFrame")? else {
            return None;
        };
        let size = vector3(dom, id, "Size")?;
        let own = part_shape(dom, id);
        let class = dom.class_name(id);
        let mut out = PartInstance {
            color: color(dom, id, "Color").unwrap_or([0.64, 0.64, 0.65]),
            transparency: number(dom, id, "Transparency").unwrap_or(0.0),
            // Confirmed: creator-docs BasePart.CastShadow, on by default;
            // places saved before it existed cast.
            cast_shadow: boolean(dom, id, "CastShadow").unwrap_or(true),
            ..PartInstance::plain(
                DrawShape::Part(own),
                cf.position,
                cf.rotation,
                own.extent(size),
            )
        };
        if class == "MeshPart" {
            // Its own mesh file, scaled by Size / InitialSize; with an empty
            // MeshId, its Size box (observed).
            let url = mesh_id(dom, id);
            if url.is_empty() {
                self.settle(id, id, Settled::Empty);
                return Some((out, Decals::Faces));
            }
            let initial = vector3(dom, id, "InitialSize").unwrap_or(Vec3::ZERO);
            let scale = mesh_part_scale(size, initial);
            match self.file(id, id, &url) {
                FileDraw::Mesh(hash) => {
                    out.shape = DrawShape::File(hash);
                    out.size = scale;
                    self.mesh_texture(dom, id, id, &mut out, [1.0; 3]);
                }
                FileDraw::Empty => {}
                FileDraw::Nothing => {
                    out.transparency = 1.0;
                    return Some((out, Decals::None));
                }
            }
            // Decals on a MeshPart are projected onto its faces (observed).
            return Some((out, Decals::Faces));
        }
        let Some((kind, mesh, url)) = part_mesh(dom, id) else {
            return Some((out, Decals::Faces));
        };
        let scale = vector3(dom, mesh, "Scale").unwrap_or(Vec3::ONE);
        let Some(mut look) = kind.look_on(class, size, scale) else {
            return Some((out, Decals::Faces));
        };
        if kind == MeshKind::EmptyFile {
            self.settle(id, mesh, Settled::Empty);
        }
        if let MeshLook::File { scale } = look {
            look = match self.file(id, mesh, &url) {
                FileDraw::Mesh(hash) => {
                    out.shape = DrawShape::File(hash);
                    out.size = scale;
                    MeshLook::File { scale }
                }
                FileDraw::Empty => MeshKind::EmptyFile
                    .look_on(class, size, scale)
                    .unwrap_or(MeshLook::PartShape),
                FileDraw::Nothing => MeshLook::Nothing,
            };
        }
        match look {
            MeshLook::Shape {
                shape,
                size,
                upright,
            } => {
                out.shape = DrawShape::Part(shape);
                out.size = size;
                if upright {
                    out.rotation = cf.rotation * UPRIGHT;
                    pose.upright = true;
                }
            }
            MeshLook::Head { scale } => {
                out.shape = DrawShape::Head;
                out.size = scale;
            }
            MeshLook::File { .. } => {}
            MeshLook::Nothing => {
                out.transparency = 1.0;
                return Some((out, Decals::None));
            }
            MeshLook::PartShape => return Some((out, Decals::Faces)),
        }
        if kind.takes_texture() {
            let tint = vector3(dom, mesh, "VertexColor").unwrap_or(Vec3::ONE);
            self.mesh_texture(dom, id, mesh, &mut out, tint.to_array());
        }
        // Moves the mesh in the part's own axes (Confirmed: creator-docs
        // DataModelMesh.Offset).
        let offset = vector3(dom, mesh, "Offset").unwrap_or(Vec3::ZERO);
        out.position += cf.rotation * offset;
        pose.offset = offset;
        let decals = if kind.decals_follow_uvs() {
            Decals::MeshUv
        } else {
            Decals::Faces
        };
        Some((out, decals))
    }

    /// A mesh's texture (`TextureId` of `mesh`, a MeshPart's `TextureID`),
    /// observed in Studio 0.742 (2026-10-13): once loaded it replaces the
    /// part's colour, times `tint` (the mesh's `VertexColor`); while it
    /// loads the mesh is black; if it fails, or there is none, the mesh
    /// keeps the part's colour.
    fn mesh_texture(
        &mut self,
        dom: &Dom,
        part: InstanceId,
        mesh: InstanceId,
        out: &mut PartInstance,
        tint: [f32; 3],
    ) {
        let url = texture_id(dom, mesh);
        if url.is_empty() {
            return;
        }
        match self.file_state(part, &url, Want::Image) {
            FileState::Loaded(hash) => {
                out.texture = Some(ImageHash(hash));
                out.color = tint;
            }
            FileState::Loading => out.color = [0.0; 3],
            FileState::Failed => {}
        }
    }

    /// A part's Decals and Textures, in child order, each drawn over `part`
    /// once its image has loaded. While an image loads, or after it fails,
    /// it draws nothing: a Decal whose `Texture` changes doesn't keep its
    /// old image (observed in Studio 0.742, 2026-10-13).
    fn images(
        &mut self,
        dom: &Dom,
        id: InstanceId,
        part: &PartInstance,
        decals: Decals,
    ) -> Vec<PartInstance> {
        let mut out = Vec::new();
        for &child in dom.children(id) {
            let class = dom.class_name(child);
            if class != "Decal" && class != "Texture" {
                continue;
            }
            let url = content_url(dom, child, &["TextureContent", "Texture"]);
            if url.is_empty() {
                continue;
            }
            let FileState::Loaded(hash) = self.file_state(id, &url, Want::Image) else {
                continue;
            };
            let face = enum_value(dom, child, "Face")
                .and_then(NormalId::from_u32)
                .unwrap_or(NormalId::Front);
            let placement = match (decals, class) {
                (Decals::MeshUv, _) => ImagePlacement::MeshUv,
                (_, "Texture") => ImagePlacement::Tiled(
                    face,
                    Tiling {
                        studs_per_tile: Vec2::new(
                            number(dom, child, "StudsPerTileU").unwrap_or(2.0),
                            number(dom, child, "StudsPerTileV").unwrap_or(2.0),
                        ),
                        offset: Vec2::new(
                            number(dom, child, "OffsetStudsU").unwrap_or(0.0),
                            number(dom, child, "OffsetStudsV").unwrap_or(0.0),
                        ),
                    },
                ),
                _ => ImagePlacement::Face(face),
            };
            out.push(PartInstance {
                color: color(dom, child, "Color3").unwrap_or([1.0; 3]),
                transparency: number(dom, child, "Transparency").unwrap_or(0.0),
                texture: None,
                layer: Layer::Image {
                    image: ImageHash(hash),
                    placement,
                    z: number(dom, child, "ZIndex").unwrap_or(1.0) as i32,
                    on_transparent: part.transparency > 0.0 && part.transparency < 1.0,
                },
                ..*part
            });
        }
        out
    }

    fn settle(&mut self, part: InstanceId, mesh: InstanceId, settled: Settled) {
        self.shown.insert(part, Shown { mesh, settled });
    }

    /// How a part's mesh file `url` draws, by the rules observed in Studio
    /// 0.742 (2026-10-13, ROADMAP M4 "File meshes (step 3)"): a new mesh
    /// draws nothing until its file arrives, and nothing if it fails to
    /// load; when a mesh's `MeshId` changes, what it drew before (a mesh, or
    /// the empty-`MeshId` box) stays until the new file arrives, and stays
    /// for good if the new one fails. The file is asked for the first time
    /// any part needs it.
    fn file(&mut self, part: InstanceId, mesh: InstanceId, url: &str) -> FileDraw {
        if let FileState::Loaded(hash) = self.file_state(part, url, Want::Mesh) {
            let hash = MeshHash(hash);
            self.settle(part, mesh, Settled::File(hash));
            return FileDraw::Mesh(hash);
        }
        match self.shown.get(&part).filter(|s| s.mesh == mesh) {
            Some(Shown {
                settled: Settled::File(hash),
                ..
            }) => FileDraw::Mesh(*hash),
            Some(Shown {
                settled: Settled::Empty,
                ..
            }) => FileDraw::Empty,
            None => FileDraw::Nothing,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noon_sun_is_high_and_midnight_below() {
        assert!(sun_direction(12.0, 41.733).y > 0.7);
        assert!(sun_direction(0.0, 41.733).y < 0.0);
        assert!(sun_direction(6.0, 0.0).y.abs() < 1e-4);
    }

    /// `Lighting:GetSunDirection()` in Studio.
    #[test]
    fn sun_matches_studio() {
        for (hours, latitude, studio) in [
            (14.0, 41.733, Vec3::new(-0.4749, 0.8225, 0.3129)),
            (9.0, 0.0, Vec3::new(0.6485, 0.6485, -0.3987)),
            (14.0, -30.0, Vec3::new(-0.2974, 0.5151, -0.8038)),
        ] {
            let d = sun_direction(hours, latitude);
            assert!(d.distance(studio) < 1e-3, "{hours} {latitude}: {d}");
        }
    }
}
