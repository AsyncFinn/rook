//! The order GUI objects stack in, bottom first, with what clips each:
//! drawing walks it upwards and hit-testing downwards
//! (docs/behavior/gui/layout.md#drawing).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::GuiQuad;

use crate::layout::{Layout, Placed, gui_children, through_folders, visible};

/// The local player's ScreenGuis that show, bottom first, then CoreGui's
/// over them all (what Roblox's CoreScripts draw). Measured: ScreenGuis
/// stack by `DisplayOrder`, later ones on top on a tie.
pub(crate) fn screens(dom: &Dom) -> Vec<InstanceId> {
    let Some(player_gui) = dom
        .local_player()
        .and_then(|p| dom.find_first_child_of_class(p, "PlayerGui"))
    else {
        return Vec::new();
    };
    let ordered = |holder: InstanceId| {
        let mut screens = through_folders(dom, holder, |id| dom.class_name(id) == "ScreenGui");
        screens.retain(|&s| dom.bool_property(s, "Enabled") != Some(false));
        screens.sort_by_key(|&s| dom.number_property(s, "DisplayOrder").unwrap_or(0.0) as i64);
        screens
    };
    let mut screens = ordered(player_gui);
    if let Some(core) = dom.find_service("CoreGui") {
        screens.extend(ordered(core));
    }
    screens
}

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
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Stacked {
    pub(crate) id: InstanceId,
    pub(crate) placed: Placed,
    pub(crate) clip: [f32; 4],
    /// An ancestor is turned (which turns clipping off below it).
    pub(crate) turned: bool,
    /// How many stacked objects hold it: in a `Sibling` stack, what an
    /// object holds is the run after it that is deeper.
    pub(crate) depth: u32,
}

/// What a LayerCollector holds, bottom first, in its `ZIndexBehavior`'s
/// order.
pub(crate) fn stack(dom: &Dom, layout: &Layout, root: InstanceId, clip: Clip) -> Vec<Stacked> {
    let mut tree = Vec::new();
    collect(dom, layout, root, (clip, 0), &mut tree, &mut |_| {});
    let mut out = Vec::new();
    if !by_sibling(dom, root) {
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

/// The visible objects `parent` holds, as a tree, each `depth` deep or
/// deeper; `hidden` is given each laid-out one that isn't (what it holds
/// is passed over).
fn collect(
    dom: &Dom,
    layout: &Layout,
    parent: InstanceId,
    (clip, depth): (Clip, u32),
    out: &mut Vec<Node>,
    hidden: &mut impl FnMut(InstanceId),
) {
    for kid in gui_children(dom, parent) {
        let Some(&placed) = layout.placed.get(&kid) else {
            continue;
        };
        if !visible(dom, kid) {
            hidden(kid);
            continue;
        }
        let stacked = Stacked {
            id: kid,
            placed,
            clip: clip.rect,
            turned: clip.turned,
            depth,
        };
        let mut kids = Vec::new();
        let inner = (inside(dom, layout, &stacked), depth + 1);
        collect(dom, layout, kid, inner, &mut kids, hidden);
        out.push(Node {
            stacked,
            z: dom.number_property(kid, "ZIndex").unwrap_or(1.0) as i64,
            kids,
        });
    }
}

/// What clips what `s` holds: what clips `s`, cut to `s` when it has
/// `ClipsDescendants` and nothing turns it.
fn inside(dom: &Dom, layout: &Layout, s: &Stacked) -> Clip {
    let (kid, placed) = (s.id, s.placed);
    let turned = s.turned || placed.rotation != 0.0;
    let mut inside = Clip {
        rect: s.clip,
        turned,
    };
    if !turned && dom.bool_property(kid, "ClipsDescendants") == Some(true) {
        // A ScrollingFrame clips to its window, not its bars.
        let (lo, hi) = match layout.scrolls.get(&kid) {
            Some(sc) => (
                placed.screen_point(sc.window_at),
                placed.screen_point(sc.window_at + sc.window),
            ),
            None => (placed.top_left(), placed.top_left() + placed.size()),
        };
        let r = s.clip;
        inside.rect = [
            r[0].max(lo.x),
            r[1].max(lo.y),
            r[2].min(hi.x),
            r[3].min(hi.y),
        ];
    }
    inside
}

/// `s` as placed now, then what it holds in sibling order (the run it
/// takes in a `Sibling` stack), and the laid-out objects in it that are
/// hidden. `None` when `s` itself is hidden.
pub(crate) fn restacked(
    dom: &Dom,
    layout: &Layout,
    s: &Stacked,
) -> Option<(Vec<Stacked>, Vec<InstanceId>)> {
    if !visible(dom, s.id) {
        return None;
    }
    let top = Stacked {
        placed: *layout.placed.get(&s.id)?,
        ..*s
    };
    let mut tree = Vec::new();
    let mut hidden = Vec::new();
    let inner = (inside(dom, layout, &top), s.depth + 1);
    collect(dom, layout, s.id, inner, &mut tree, &mut |id| {
        hidden.push(id)
    });
    let mut out = vec![top];
    sibling_order(tree, &mut out);
    Some((out, hidden))
}

/// Whether `root`'s stack puts each object with all it holds over it,
/// so what an object holds is one run right after it.
pub(crate) fn by_sibling(dom: &Dom, root: InstanceId) -> bool {
    dom.enum_item_property(root, "ZIndexBehavior") != Some("Global")
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
