//! Local lights: PointLight, SpotLight and SurfaceLight under parts and
//! their Attachments in Workspace (docs/behavior/lights.md).

use rook_render::{LightShape, LocalLight};

use super::*;

/// Whether `id` is a local light.
fn is_light(dom: &Dom, id: InstanceId) -> bool {
    dom.is_a(id, "Light")
}

/// Every local light in Workspace, drawn or not (its look is read again
/// whenever a light changes; see [`SceneCache::lights_changed`]).
fn find_lights(dom: &Dom) -> Vec<InstanceId> {
    let Some(ws) = dom.find_service("Workspace") else {
        return Vec::new();
    };
    let mut lights = dom.descendants(ws);
    lights.retain(|id| is_light(dom, *id));
    lights
}

/// A light as it shines now: under a part, from the part (a surface
/// light from the part's `Face`); under an Attachment, from the
/// attachment, facing along its axes. `None` when it isn't under either
/// (Studio lights nothing then). A disabled light shines with brightness
/// 0, so that scripts switching lights on and off (alarms) change only
/// their colours.
fn light(dom: &Dom, id: InstanceId) -> Option<LocalLight> {
    let enabled = dom.bool_property(id, "Enabled") != Some(false);
    let parent = dom.parent(id)?;
    let part = if is_part(dom, parent) {
        parent
    } else if dom.class_name(parent) == "Attachment" {
        dom.parent(parent).filter(|&p| is_part(dom, p))?
    } else {
        return None;
    };
    let part_cf = dom.cframe_property(part, "CFrame")?;
    let part_size = dom.vector3_property(part, "Size").unwrap_or(Vec3::ONE);
    let (cf, size) = if part == parent {
        (part_cf, part_size)
    } else {
        (part_cf * dom.cframe_property(parent, "CFrame")?, Vec3::ZERO)
    };
    let r = part_cf.rotation;
    let extent =
        Mat3::from_cols(r.x_axis.abs(), r.y_axis.abs(), r.z_axis.abs()) * (part_size * 0.5);
    // SpotLight and SurfaceLight face Front (-Z) by default.
    let face = dom
        .enum_property(id, "Face")
        .and_then(NormalId::from_u32)
        .unwrap_or(NormalId::Front);
    let direction = cf.rotation * face.normal();
    let angle = dom.float_property(id, "Angle").unwrap_or(90.0);
    let (position, shape) = match dom.class_name(id) {
        "PointLight" => (cf.position, LightShape::Point),
        // Measured: a spot light shines from its part's centre.
        "SpotLight" => (cf.position, LightShape::Spot { direction, angle }),
        "SurfaceLight" => {
            // From the centre of the part's face.
            let n = face.normal();
            let out = n * n.abs().dot(size * 0.5);
            (
                cf.position + cf.rotation * out,
                LightShape::Surface { direction, angle },
            )
        }
        _ => return None,
    };
    Some(LocalLight {
        position,
        shape,
        color: dom
            .color3_property(id, "Color")
            .map(Color3::to_array)
            .unwrap_or([1.0; 3]),
        brightness: if enabled {
            dom.float_property(id, "Brightness").unwrap_or(1.0)
        } else {
            0.0
        },
        range: dom.float_property(id, "Range").unwrap_or(8.0),
        shadows: dom.bool_property(id, "Shadows").unwrap_or(false),
        part: (part_cf.position - extent, part_cf.position + extent),
    })
}

impl SceneCache {
    /// Find every local light in Workspace again, and read them.
    pub(crate) fn find_lights(&mut self, dom: &Dom) {
        profiling::scope!("SceneCache::find_lights");
        self.light_ids = find_lights(dom);
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
        light(dom, self.light_ids[i]).unwrap_or(LocalLight {
            position: Vec3::ZERO,
            shape: LightShape::Point,
            color: [0.0; 3],
            brightness: 0.0,
            range: 0.0,
            shadows: false,
            part: (Vec3::ZERO, Vec3::ZERO),
        })
    }

    /// Bring the lights up to date with a frame's changes, reading again
    /// only the lights that changed and those on parts and Attachments
    /// that did; whether any may have changed.
    pub(crate) fn lights_changed(&mut self, dom: &Dom, changes: &RenderChanges) -> bool {
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
}
