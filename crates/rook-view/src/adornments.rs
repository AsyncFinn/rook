//! Adornments drawn on a part (docs/behavior/rendering/adornments.md), as Studio
//! 0.742 draws them: SelectionBox, SelectionSphere, SurfaceSelection,
//! the HandleAdornments, Handles, ArcHandles, FloorWire and
//! SelectionPartLasso, each kind in its own file, and the Highlight
//! covering a part. They draw as extra entries riding along with their
//! Adornee (a FloorWire's `From`, a lasso's Humanoid's root part).

pub(super) mod floor_wire;
mod handle_adornment;
mod handles;
pub(super) mod highlight;
mod lasso;
mod selection_box;
mod selection_sphere;
mod surface_selection;
pub(super) mod wireframe;

use rook_render::{AdornShape, Grow, ImageHash, Light};

use super::*;
use wireframe::WireLines;

/// The default `Color3` of most adornment kinds here, (13, 105, 172)
/// (observed).
const DEFAULT_COLOR: [f32; 3] = [13.0 / 255.0, 105.0 / 255.0, 172.0 / 255.0];

/// What drawing an adornment needs besides the DOM: the lines scripts gave
/// WireframeHandleAdornments, and a way to load an image (`None` until it
/// has loaded; failed ones load as `Some(None)`).
pub(super) struct Context<'a> {
    pub(super) wires: &'a WireLines,
    pub(super) image: &'a mut dyn FnMut(&str) -> Option<Option<ImageHash>>,
}

impl SceneCache {
    /// The adornments on part `id`, with their poses in the part's axes
    /// (and any growth anchor there, placed in the world here).
    /// Observed in Studio 0.742: an adornment draws only on its `Adornee`
    /// (one merely inside a part draws nothing), wherever it is in
    /// Workspace.
    pub(super) fn adornments(&mut self, dom: &Dom, id: InstanceId) -> Vec<(PartInstance, Pose)> {
        let wires = std::mem::take(&mut self.wire_lines);
        let mut image = |url: &str| match self.file_state(id, url, Want::Image) {
            FileState::Loaded(hash) => Some(Some(ImageHash(hash))),
            FileState::Failed => Some(None),
            FileState::Loading => None,
        };
        let mut cx = Context {
            wires: &wires,
            image: &mut image,
        };
        let out = adornments(dom, id, &mut cx);
        self.wire_lines = wires;
        out
    }
}

fn adornments(dom: &Dom, id: InstanceId, cx: &mut Context<'_>) -> Vec<(PartInstance, Pose)> {
    let (Some(size), Some(cf)) = (dom.vector3_property(id, "Size"), dom.drawn_cframe(id)) else {
        return Vec::new();
    };
    let mut holders: Vec<InstanceId> = Vec::new();
    let lassos = rook_lua::humanoid_of_root(dom, id)
        .map(|h| dom.referrers(h))
        .unwrap_or_default();
    for &(holder, _) in dom.referrers(id).iter().chain(lassos) {
        if rook_engine::adorned_part(dom, holder) == Some(id)
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
            "Handles" => handles::handles(dom, holder, size),
            "ArcHandles" => handles::arc_handles(dom, holder, size),
            "FloorWire" => floor_wire::floor_wire(dom, holder, cf),
            "SelectionPartLasso" => lasso::lasso(dom, holder, cf),
            "WireframeHandleAdornment" => wireframe::wireframe(dom, holder, size, cx.wires),
            _ if dom.is_a(holder, "HandleAdornment") => {
                handle_adornment::handle_adornment(dom, holder, size, cx)
            }
            _ => Vec::new(),
        });
    }
    for (part, pose) in &mut out {
        part.position = cf.position + cf.rotation * pose.offset;
        part.rotation = cf.rotation * pose.turn;
        if let Some(Grow::Whole { anchor, .. }) = &mut part.grows {
            *anchor = cf.position + cf.rotation * *anchor;
        }
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

/// A pose `offset` from the part's center, unturned.
fn at(offset: Vec3) -> Pose {
    Pose {
        offset,
        turn: Mat3::IDENTITY,
    }
}
