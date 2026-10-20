//! Adornments drawn on a part (docs/behavior/adornments.md), as Studio
//! 0.742 draws them: SelectionBox, SelectionSphere, SurfaceSelection and
//! the Box, Sphere, Cylinder and Cone HandleAdornments, each kind in its
//! own file. They draw as extra entries riding along with their Adornee.

mod handle_adornment;
mod selection_box;
mod selection_sphere;
mod surface_selection;

use rook_render::{AdornShape, Light};

use super::*;

/// The default `Color3` of every adornment kind here, (13, 105, 172)
/// (observed).
const DEFAULT_COLOR: [f32; 3] = [13.0 / 255.0, 105.0 / 255.0, 172.0 / 255.0];

/// The adornments on part `id`, with their poses in the part's axes.
/// Observed in Studio 0.742: an adornment draws only on its `Adornee`
/// (one merely inside a part draws nothing), wherever it is in Workspace.
pub(super) fn adornments(dom: &Dom, id: InstanceId) -> Vec<(PartInstance, Pose)> {
    let (Some(size), Some(cf)) = (
        dom.vector3_property(id, "Size"),
        dom.cframe_property(id, "CFrame"),
    ) else {
        return Vec::new();
    };
    let mut holders: Vec<InstanceId> = Vec::new();
    for &(holder, _) in dom.referrers(id) {
        if dom.ref_property(holder, "Adornee") == Some(id)
            && !holders.contains(&holder)
            && shown(dom, holder)
        {
            holders.push(holder);
        }
    }
    let mut out = Vec::new();
    for holder in holders {
        out.extend(match dom.class_name(holder) {
            "SelectionBox" => selection_box::selection_box(dom, holder, size),
            "SelectionSphere" => selection_sphere::selection_sphere(dom, holder, size),
            "SurfaceSelection" => surface_selection::surface_selection(dom, holder, size),
            _ if dom.is_a(holder, "HandleAdornment") => {
                handle_adornment::handle_adornment(dom, holder, size)
            }
            _ => Vec::new(),
        });
    }
    for (part, pose) in &mut out {
        part.position = cf.position + cf.rotation * pose.offset;
        part.rotation = cf.rotation * pose.turn;
    }
    out
}

/// Whether adornment `id` draws: `Visible`, and somewhere drawn from
/// (Workspace, or a PlayerGui).
fn shown(dom: &Dom, id: InstanceId) -> bool {
    if dom.bool_property(id, "Visible") == Some(false) {
        return false;
    }
    let mut at = dom.parent(id);
    while let Some(p) = at {
        if matches!(dom.class_name(p), "Workspace" | "PlayerGui") {
            return true;
        }
        at = dom.parent(p);
    }
    false
}

/// A piece of adornment `id` of `shape`: its `Color3` at its
/// `Transparency`, without light, casting no shadow.
fn piece(dom: &Dom, id: InstanceId, shape: DrawShape) -> PartInstance {
    PartInstance {
        color: color(dom, id, "Color3"),
        transparency: dom.float_property(id, "Transparency").unwrap_or(0.0),
        cast_shadow: false,
        light: Light::Unlit,
        ..PartInstance::plain(shape, Vec3::ZERO, Mat3::IDENTITY, Vec3::ONE)
    }
}

fn color(dom: &Dom, id: InstanceId, property: &str) -> [f32; 3] {
    dom.color3_property(id, property)
        .map(Color3::to_array)
        .unwrap_or(DEFAULT_COLOR)
}

/// A pose `offset` from the part's centre, unturned.
fn at(offset: Vec3) -> Pose {
    Pose {
        offset,
        turn: Mat3::IDENTITY,
    }
}
