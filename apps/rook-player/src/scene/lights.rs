//! Local lights: PointLight, SpotLight and SurfaceLight under parts and
//! their Attachments in Workspace (docs/behavior/lights.md).

use rook_render::{LightShape, LocalLight};

use super::*;

/// Whether `id` is a local light.
pub(super) fn is_light(dom: &Dom, id: InstanceId) -> bool {
    matches!(
        dom.class_name(id),
        "PointLight" | "SpotLight" | "SurfaceLight"
    )
}

/// Every local light in Workspace, drawn or not (its look is read again
/// whenever a light changes; see [`SceneCache::lights_changed`]).
pub(super) fn find_lights(dom: &Dom) -> Vec<InstanceId> {
    let Some(ws) = dom.find_service("Workspace") else {
        return Vec::new();
    };
    let mut lights = dom.descendants(ws);
    lights.retain(|id| is_light(dom, *id));
    lights
}

fn cframe(dom: &Dom, id: InstanceId) -> Option<CFrame> {
    match dom.property_by_name(id, "CFrame")? {
        Value::CFrame(cf) => Some(cf),
        _ => None,
    }
}

/// A light as it shines now: under a part, from the part (a surface
/// light from the part's `Face`); under an Attachment, from the
/// attachment, facing along its axes. `None` when it isn't under either
/// (Studio lights nothing then). A disabled light shines with brightness
/// 0, so that scripts switching lights on and off (alarms) change only
/// their colours.
pub(super) fn light(dom: &Dom, id: InstanceId) -> Option<LocalLight> {
    let enabled = boolean(dom, id, "Enabled") != Some(false);
    let parent = dom.parent(id)?;
    let (cf, size) = if is_part(dom, parent) {
        let size = vector3(dom, parent, "Size").unwrap_or(Vec3::ONE);
        (cframe(dom, parent)?, size)
    } else if dom.class_name(parent) == "Attachment" {
        let part = dom.parent(parent)?;
        if !is_part(dom, part) {
            return None;
        }
        (cframe(dom, part)? * cframe(dom, parent)?, Vec3::ZERO)
    } else {
        return None;
    };
    // SpotLight and SurfaceLight face Front (-Z) by default.
    let face = enum_value(dom, id, "Face")
        .and_then(NormalId::from_u32)
        .unwrap_or(NormalId::Front);
    let direction = cf.rotation * face.normal();
    let angle = number(dom, id, "Angle").unwrap_or(90.0);
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
        color: color(dom, id, "Color").unwrap_or([1.0; 3]),
        brightness: if enabled {
            number(dom, id, "Brightness").unwrap_or(1.0)
        } else {
            0.0
        },
        range: number(dom, id, "Range").unwrap_or(8.0),
        shadows: boolean(dom, id, "Shadows").unwrap_or(false),
    })
}
