//! Build a render [`Scene`] from a DataModel.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Mat3, Vec3};
use rook_content::{ContentRef, Resolver};
use rook_dom::{Dom, InstanceId};
use rook_engine::{DataModel, RenderChanges};
use rook_physics::{part_shape, workspace_parts};
use rook_render::{Camera, DrawShape, Lighting, MeshHash, PartInstance, Scene};
use rook_types::{Color3, MeshKind, MeshLook, Value, mesh_part_scale};

use crate::meshes::{MeshFiles, MeshState};

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

fn vector3(dom: &Dom, id: InstanceId, name: &str) -> Option<Vec3> {
    match dom.property_by_name(id, name)? {
        Value::Vector3(v) => Some(v.0),
        _ => None,
    }
}

/// The mesh child that decides how a part looks: of several, the one added
/// last (observed: a part with a SpecialMesh Sphere and a CylinderMesh drew
/// whichever was parented second). With its kind and `MeshId`.
fn part_mesh(dom: &Dom, id: InstanceId) -> Option<(MeshKind, InstanceId, String)> {
    dom.children(id).iter().rev().find_map(|&child| {
        let mesh_type = match dom.property_by_name(child, "MeshType") {
            Some(Value::Enum(n)) => Some(n),
            _ => None,
        };
        let mesh_id = mesh_id(dom, child);
        let kind = MeshKind::of(dom.class_name(child), mesh_type, &mesh_id)?;
        Some((kind, child, mesh_id))
    })
}

/// A mesh's or MeshPart's `MeshId`. Files store it in `MeshContent` (`MeshId`
/// is a view of it); some 2016 MeshParts saved it as `MeshID`.
fn mesh_id(dom: &Dom, id: InstanceId) -> String {
    ["MeshContent", "MeshID"]
        .iter()
        .find_map(|name| {
            dom.property_by_name(id, name)?
                .content_url()
                .filter(|u| !u.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_default()
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

/// Direction to the sun for a clock time and latitude, from the standard
/// solar position formula with zero declination. Which world axis is east
/// is Inference until checked against `Lighting:GetSunDirection()` in Studio.
pub fn sun_direction(hours: f32, latitude_degrees: f32) -> Vec3 {
    let h = (hours - 12.0) / 24.0 * std::f32::consts::TAU;
    let lat = latitude_degrees.to_radians();
    let up = lat.cos() * h.cos();
    let east = -h.sin();
    let north = -lat.sin() * h.cos();
    // East = +X, north = -Z (Inference).
    Vec3::new(east, up, -north).normalize_or(Vec3::Y)
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
    // A night sky darkens towards black as the sun sets (Approximation
    // until skyboxes are supported).
    let day = (l.sun_direction.y * 4.0 + 0.5).clamp(0.05, 1.0);
    l.sky_top = l.sky_top.map(|c| c * day);
    l.sky_horizon = l.sky_horizon.map(|c| c * day);
    l
}

/// The render scene for a DataModel, kept between frames so that only the
/// parts scripts changed are rebuilt.
#[derive(Debug)]
pub struct SceneCache {
    pub scene: Scene,
    /// Where each drawn part is in `scene.parts`.
    index: HashMap<InstanceId, usize>,
    /// Mesh files, loaded as parts need them.
    files: MeshFiles,
    /// Parts waiting for each mesh file, redrawn when it arrives.
    waiting: HashMap<ContentRef, HashSet<InstanceId>>,
    /// What each part's mesh file last drew.
    shown: HashMap<InstanceId, Shown>,
}

impl SceneCache {
    /// Mesh files resolve through `content`.
    pub fn new(dom: &Dom, content: Arc<Resolver>) -> Self {
        let mut cache = Self {
            scene: Scene::default(),
            index: HashMap::new(),
            files: MeshFiles::new(content),
            waiting: HashMap::new(),
            shown: HashMap::new(),
        };
        cache.rebuild(dom);
        cache
    }

    /// Rebuild everything (parts added, removed or moved).
    pub fn rebuild(&mut self, dom: &Dom) {
        profiling::scope!("SceneCache::rebuild");
        self.scene.parts.clear();
        self.index.clear();
        for id in workspace_parts(dom) {
            if let Some(p) = self.part(dom, id) {
                self.index.insert(id, self.scene.parts.len());
                self.scene.parts.push(p);
            }
        }
        let index = &self.index;
        self.shown.retain(|id, _| index.contains_key(id));
        self.scene.lighting = lighting(dom);
    }

    /// Apply a frame's changes. Returns whether anything changed.
    pub fn apply(&mut self, dom: &Dom, changes: &RenderChanges) -> bool {
        profiling::scope!("SceneCache::apply");
        if changes.full {
            self.rebuild(dom);
            return true;
        }
        if changes.lighting {
            self.scene.lighting = lighting(dom);
        }
        let mut changed = changes.lighting;
        for id in &changes.parts {
            // Parts outside Workspace aren't drawn.
            let Some(&i) = self.index.get(id) else {
                continue;
            };
            match self.part(dom, *id) {
                Some(p) => self.scene.parts[i] = p,
                None => {
                    self.rebuild(dom);
                    return true;
                }
            }
            changed = true;
        }
        changed
    }

    /// Take the mesh files that finished loading and redraw the parts
    /// waiting for them; with `wait`, first wait until none is loading.
    /// Returns whether anything changed.
    pub fn poll_meshes(&mut self, dom: &Dom, wait: bool) -> bool {
        let finished = self.files.poll(wait);
        if finished.is_empty() {
            return false;
        }
        profiling::scope!("SceneCache::poll_meshes");
        let mut parts = HashSet::new();
        for (r, loaded) in finished {
            if let Some((hash, mesh)) = loaded {
                self.scene.meshes.insert(hash, mesh);
            }
            parts.extend(self.waiting.remove(&r).unwrap_or_default());
        }
        for id in parts {
            let Some(&i) = self.index.get(&id) else {
                continue;
            };
            if let Some(p) = self.part(dom, id) {
                self.scene.parts[i] = p;
            }
        }
        true
    }

    /// What a part draws (`None` without a CFrame or Size). A mesh that
    /// draws nothing leaves the part fully transparent, so it stays in the
    /// scene for a script to change back.
    fn part(&mut self, dom: &Dom, id: InstanceId) -> Option<PartInstance> {
        let Value::CFrame(cf) = dom.property_by_name(id, "CFrame")? else {
            return None;
        };
        let size = vector3(dom, id, "Size")?;
        let own = part_shape(dom, id);
        let mut out = PartInstance {
            shape: DrawShape::Part(own),
            position: cf.position,
            rotation: cf.rotation,
            size: own.extent(size),
            color: color(dom, id, "Color").unwrap_or([0.64, 0.64, 0.65]),
            transparency: number(dom, id, "Transparency").unwrap_or(0.0),
        };
        let class = dom.class_name(id);
        if class == "MeshPart" {
            // Its own mesh file, scaled by Size / InitialSize; with an empty
            // MeshId, its Size box (observed).
            let url = mesh_id(dom, id);
            if url.is_empty() {
                self.settle(id, id, Settled::Empty);
                return Some(out);
            }
            let initial = vector3(dom, id, "InitialSize").unwrap_or(Vec3::ZERO);
            let scale = mesh_part_scale(size, initial);
            match self.file(id, id, &url) {
                FileDraw::Mesh(hash) => {
                    out.shape = DrawShape::File(hash);
                    out.size = scale;
                }
                FileDraw::Empty => {}
                FileDraw::Nothing => out.transparency = 1.0,
            }
            return Some(out);
        }
        let Some((kind, mesh, url)) = part_mesh(dom, id) else {
            return Some(out);
        };
        let scale = vector3(dom, mesh, "Scale").unwrap_or(Vec3::ONE);
        let Some(mut look) = kind.look_on(class, size, scale) else {
            return Some(out);
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
                }
            }
            MeshLook::Head { scale } => {
                out.shape = DrawShape::Head;
                out.size = scale;
            }
            MeshLook::File { .. } => {}
            MeshLook::Nothing => {
                out.transparency = 1.0;
                return Some(out);
            }
            MeshLook::PartShape => return Some(out),
        }
        // Moves the mesh in the part's own axes (Confirmed: creator-docs
        // DataModelMesh.Offset).
        let offset = vector3(dom, mesh, "Offset").unwrap_or(Vec3::ZERO);
        out.position += cf.rotation * offset;
        Some(out)
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
        let state = match ContentRef::parse(url) {
            Some(r) => {
                let state = self.files.state(&r);
                if state == MeshState::Loading {
                    self.waiting.entry(r).or_default().insert(part);
                }
                state
            }
            None => MeshState::Failed,
        };
        if let MeshState::Loaded(hash) = state {
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
}
