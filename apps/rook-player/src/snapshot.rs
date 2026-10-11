//! Build a render [`Scene`] from a DataModel.

use std::collections::HashMap;

use glam::{Mat3, Vec3};
use rook_dom::{Dom, InstanceId};
use rook_engine::RenderChanges;
use rook_render::{Camera, Lighting, PartInstance, Scene, Shape};
use rook_types::Value;

fn number(dom: &Dom, id: InstanceId, name: &str) -> Option<f32> {
    match dom.property_by_name(id, name)? {
        Value::Float32(n) => Some(n),
        Value::Float64(n) => Some(n as f32),
        Value::Int32(n) => Some(n as f32),
        _ => None,
    }
}

fn color(dom: &Dom, id: InstanceId, name: &str) -> Option<[f32; 3]> {
    match dom.property_by_name(id, name)? {
        Value::Color3(c) => Some([c.r, c.g, c.b]),
        Value::Color3uint8(c) => Some([c.r, c.g, c.b].map(|x| f32::from(x) / 255.0)),
        _ => None,
    }
}

/// `Enum.PartType`: Ball 0, Block 1, Cylinder 2, Wedge 3, CornerWedge 4.
fn shape(dom: &Dom, id: InstanceId) -> Shape {
    match dom.class_name(id) {
        "WedgePart" => Shape::Wedge,
        "CornerWedgePart" => Shape::CornerWedge,
        _ => match dom.property_by_name(id, "Shape") {
            Some(Value::Enum(0)) => Shape::Ball,
            Some(Value::Enum(2)) => Shape::Cylinder,
            Some(Value::Enum(3)) => Shape::Wedge,
            Some(Value::Enum(4)) => Shape::CornerWedge,
            // MeshParts, unions and trusses draw as their bounding box
            // until meshes are supported.
            _ => Shape::Block,
        },
    }
}

fn part(dom: &Dom, id: InstanceId) -> Option<PartInstance> {
    let Value::CFrame(cf) = dom.property_by_name(id, "CFrame")? else {
        return None;
    };
    let Value::Vector3(size) = dom.property_by_name(id, "Size")? else {
        return None;
    };
    let shape = shape(dom, id);
    let mut size = size.0;
    match shape {
        // Balls are spheres of the smallest dimension; cylinders run along X
        // with the smaller of Y and Z as diameter (Confirmed: creator-docs
        // PartType).
        Shape::Ball => size = Vec3::splat(size.min_element()),
        Shape::Cylinder => {
            let d = size.y.min(size.z);
            size = Vec3::new(size.x, d, d);
        }
        _ => {}
    }
    Some(PartInstance {
        shape,
        position: cf.position,
        rotation: cf.rotation,
        size,
        color: color(dom, id, "Color").unwrap_or([0.64, 0.64, 0.65]),
        transparency: number(dom, id, "Transparency").unwrap_or(0.0),
    })
}

/// Hours on Lighting's clock.
fn clock_time(dom: &Dom, lighting: InstanceId) -> f32 {
    if let Some(Value::String(t)) = dom.property_by_name(lighting, "TimeOfDay") {
        let mut parts = t.split(':').map(|p| p.trim().parse::<f32>().unwrap_or(0.0));
        let h = parts.next().unwrap_or(14.0);
        let m = parts.next().unwrap_or(0.0);
        let s = parts.next().unwrap_or(0.0);
        return (h + m / 60.0 + s / 3600.0).rem_euclid(24.0);
    }
    14.0
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
    let hours = clock_time(dom, id);
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
#[derive(Debug, Default)]
pub struct SceneCache {
    pub scene: Scene,
    /// Where each drawn part is in `scene.parts`.
    index: HashMap<InstanceId, usize>,
}

impl SceneCache {
    pub fn new(dom: &Dom) -> Self {
        let mut cache = Self::default();
        cache.rebuild(dom);
        cache
    }

    /// Rebuild everything (parts added, removed or moved).
    pub fn rebuild(&mut self, dom: &Dom) {
        profiling::scope!("SceneCache::rebuild");
        self.scene.parts.clear();
        self.index.clear();
        if let Some(ws) = dom.find_service("Workspace") {
            for id in dom.descendants(ws) {
                if dom.is_a(id, "BasePart")
                    && dom.class_name(id) != "Terrain"
                    && let Some(p) = part(dom, id)
                {
                    self.index.insert(id, self.scene.parts.len());
                    self.scene.parts.push(p);
                }
            }
        }
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
            match part(dom, *id) {
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
}

/// The camera the place was saved with, or a view of the origin.
pub fn initial_camera(dom: &Dom) -> Camera {
    let saved = dom.find_service("Workspace").and_then(|ws| {
        let Value::Ref(Some(r)) = dom.property_by_name(ws, "CurrentCamera")? else {
            return None;
        };
        let cam = InstanceId::from_ref(r);
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
