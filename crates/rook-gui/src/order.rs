//! The order GUI objects stack in, bottom first, with what clips each:
//! drawing walks it upwards and hit-testing downwards
//! (docs/behavior/gui/layout.md#drawing).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::GuiQuad;

use crate::layout::{Layout, Placed, gui_children, through_folders, visible};

/// The local player's ScreenGuis that show, bottom first. Measured:
/// ScreenGuis stack by `DisplayOrder`, later ones on top on a tie.
pub(crate) fn screens(dom: &Dom) -> Vec<InstanceId> {
    let Some(player_gui) = dom
        .local_player()
        .and_then(|p| dom.find_first_child_of_class(p, "PlayerGui"))
    else {
        return Vec::new();
    };
    let mut screens = through_folders(dom, player_gui, |id| dom.class_name(id) == "ScreenGui");
    screens.retain(|&s| dom.bool_property(s, "Enabled") != Some(false));
    screens.sort_by_key(|&s| dom.number_property(s, "DisplayOrder").unwrap_or(0.0) as i64);
    screens
}

/// What clips an object's descendants.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Clip {
    pub(crate) rect: [f32; 4],
    /// The object or an ancestor is turned, which turns clipping off
    /// below it (Confirmed: creator-docs GuiObject.ClipsDescendants).
    pub(crate) turned: bool,
}

impl Clip {
    pub(crate) const NONE: Self = Self {
        rect: GuiQuad::NO_CLIP,
        turned: false,
    };
}

/// What clips a world GUI's canvas of `size` pixels: its edges, with
/// `ClipsDescendants`.
pub(crate) fn canvas_clip(dom: &Dom, gui: InstanceId, size: Vec2) -> Clip {
    Clip {
        rect: if dom.bool_property(gui, "ClipsDescendants") == Some(true) {
            [0.0, 0.0, size.x, size.y]
        } else {
            GuiQuad::NO_CLIP
        },
        turned: false,
    }
}

/// How far away a world GUI still shows, and takes the mouse.
/// Inference: a SurfaceGui's default `MaxDistance` 0 means no limit, as
/// creator-docs say.
pub(crate) fn max_distance(dom: &Dom, gui: InstanceId) -> f32 {
    match dom.float_property(gui, "MaxDistance") {
        Some(d) if d > 0.0 => d,
        _ => f32::INFINITY,
    }
}

/// A visible object in the stack: where it is and the rectangle its
/// ancestors clip it to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Stacked {
    pub(crate) id: InstanceId,
    pub(crate) placed: Placed,
    pub(crate) clip: [f32; 4],
}

/// What a LayerCollector holds, bottom first, in its `ZIndexBehavior`'s
/// order.
pub(crate) fn stack(dom: &Dom, layout: &Layout, root: InstanceId, clip: Clip) -> Vec<Stacked> {
    let mut tree = Vec::new();
    collect(dom, layout, root, clip, &mut tree);
    let mut out = Vec::new();
    if dom.enum_item_property(root, "ZIndexBehavior") == Some("Global") {
        // Measured: every object by ZIndex alone, later in the tree on
        // top on a tie.
        let mut flat = Vec::new();
        flatten(tree, &mut flat);
        flat.sort_by_key(|n| n.z);
        out.extend(flat.into_iter().map(|n| n.stacked));
    } else {
        sibling_order(tree, &mut out);
    }
    out
}

struct Node {
    stacked: Stacked,
    z: i64,
    kids: Vec<Node>,
}

fn collect(dom: &Dom, layout: &Layout, parent: InstanceId, clip: Clip, out: &mut Vec<Node>) {
    for kid in gui_children(dom, parent) {
        let Some(&placed) = layout.placed.get(&kid) else {
            continue;
        };
        if !visible(dom, kid) {
            continue;
        }
        let turned = clip.turned || placed.rotation != 0.0;
        let mut inside = Clip {
            rect: clip.rect,
            turned,
        };
        if !turned && dom.bool_property(kid, "ClipsDescendants") == Some(true) {
            // A ScrollingFrame clips to its window, not its bars.
            let (lo, hi) = match layout.scrolls.get(&kid) {
                Some(s) => (
                    placed.screen_point(s.window_at),
                    placed.screen_point(s.window_at + s.window),
                ),
                None => (placed.top_left(), placed.top_left() + placed.size()),
            };
            let r = clip.rect;
            inside.rect = [
                r[0].max(lo.x),
                r[1].max(lo.y),
                r[2].min(hi.x),
                r[3].min(hi.y),
            ];
        }
        let mut kids = Vec::new();
        collect(dom, layout, kid, inside, &mut kids);
        out.push(Node {
            stacked: Stacked {
                id: kid,
                placed,
                clip: clip.rect,
            },
            z: dom.number_property(kid, "ZIndex").unwrap_or(1.0) as i64,
            kids,
        });
    }
}

fn flatten(nodes: Vec<Node>, out: &mut Vec<Node>) {
    for mut n in nodes {
        let kids = std::mem::take(&mut n.kids);
        out.push(n);
        flatten(kids, out);
    }
}

/// Measured: siblings by ZIndex (later on top on a tie), each with all it
/// holds over it.
fn sibling_order(mut nodes: Vec<Node>, out: &mut Vec<Stacked>) {
    nodes.sort_by_key(|n| n.z);
    for n in nodes {
        out.push(n.stacked);
        sibling_order(n.kids, out);
    }
}
