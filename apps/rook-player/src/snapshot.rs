//! Build a render [`Scene`] from a DataModel.

use std::collections::HashMap;

use glam::{Mat3, Vec3};
use rook_dom::{Dom, InstanceId};
use rook_engine::raycast::{part_shape, workspace_parts};
use rook_engine::{DataModel, RenderChanges};
use rook_render::{Camera, Lighting, PartInstance, Scene};
use rook_types::{Color3, Value};

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

fn part(dom: &Dom, id: InstanceId) -> Option<PartInstance> {
    let Value::CFrame(cf) = dom.property_by_name(id, "CFrame")? else {
        return None;
    };
    let Value::Vector3(size) = dom.property_by_name(id, "Size")? else {
        return None;
    };
    let shape = part_shape(dom, id);
    let size = shape.extent(size.0);
    Some(PartInstance {
        shape,
        position: cf.position,
        rotation: cf.rotation,
        size,
        color: color(dom, id, "Color").unwrap_or([0.64, 0.64, 0.65]),
        transparency: number(dom, id, "Transparency").unwrap_or(0.0),
    })
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
        for id in workspace_parts(dom) {
            if let Some(p) = part(dom, id) {
                self.index.insert(id, self.scene.parts.len());
                self.scene.parts.push(p);
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
