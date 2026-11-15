//! Where an object sits for layout: its GUI parent through Folders, the
//! top of its tree, whether that's on the screen, and the screen's size.

use super::*;

/// The GUI object or LayerCollector placing `id`: its parent, past
/// Folders.
pub(super) fn gui_parent(dom: &Dom, id: InstanceId) -> Option<InstanceId> {
    let mut at = dom.parent(id);
    while let Some(p) = at {
        if dom.class_name(p) != "Folder" {
            return dom.is_a(p, "GuiBase2d").then_some(p);
        }
        at = dom.parent(p);
    }
    None
}

/// The top of the GUI tree holding `id` (a GUI object, or a UI component
/// in one): its LayerCollector, or its topmost GUI ancestor.
pub(crate) fn top_of(dom: &Dom, id: InstanceId) -> Option<InstanceId> {
    top_along(dom, std::iter::successors(Some(id), |&i| dom.parent(i)))
}

/// The top of the GUI tree along `chain`, an instance then its ancestors
/// (as [`top_of`] climbs them).
pub(crate) fn top_along(
    dom: &Dom,
    chain: impl IntoIterator<Item = InstanceId>,
) -> Option<InstanceId> {
    let mut top = None;
    for i in chain {
        if world::is_world_gui(dom, i) {
            // Its own tree, even inside a ScreenGui (measured: the prompt
            // GUI Roblox makes hangs in one).
            return Some(i);
        } else if dom.is_a(i, "GuiBase2d") {
            top = Some(i);
        } else if dom.class_name(i) != "Folder" && !dom.is_a(i, "UIComponent") {
            break;
        }
    }
    top
}

/// Whether [`Layout::screens`] lays out the tree from `top` as a screen's:
/// it's under a player's PlayerGui, StarterGui or CoreGui, through
/// Folders.
pub(crate) fn on_screen(dom: &Dom, top: InstanceId) -> bool {
    if !dom.is_a(top, "GuiBase2d") {
        return false;
    }
    let mut holder = dom.parent(top);
    while let Some(h) = holder.filter(|&h| dom.class_name(h) == "Folder") {
        holder = dom.parent(h);
    }
    let Some(holder) = holder else {
        return false;
    };
    match dom.class_name(holder) {
        "StarterGui" | "CoreGui" => dom.parent(holder) == Some(dom.root()),
        "PlayerGui" => {
            let player = dom.parent(holder);
            let players = player.and_then(|p| dom.parent(p));
            players.is_some()
                && players == dom.find_service("Players")
                && player.and_then(|p| dom.find_first_child_of_class(p, "PlayerGui"))
                    == Some(holder)
        }
        _ => false,
    }
}

/// The window's size: the current camera's `ViewportSize`.
pub(crate) fn screen_size(dom: &Dom) -> Vec2 {
    dom.find_service("Workspace")
        .and_then(|ws| dom.ref_property(ws, "CurrentCamera"))
        .and_then(|c| dom.vector2_property(c, "ViewportSize"))
        .map_or(Vec2::ZERO, |v| v.0)
}

/// `parent`'s children that pass `keep`, with Folders' children in their
/// place (measured: a Frame in a Folder is placed in the Folder's parent).
pub(crate) fn through_folders(
    dom: &Dom,
    parent: InstanceId,
    keep: impl Fn(InstanceId) -> bool + Copy,
) -> Vec<InstanceId> {
    let mut out = Vec::new();
    for &c in dom.children(parent) {
        if dom.class_name(c) == "Folder" {
            out.extend(through_folders(dom, c, keep));
        } else if keep(c) {
            out.push(c);
        }
    }
    out
}

pub(crate) fn gui_children(dom: &Dom, parent: InstanceId) -> Vec<InstanceId> {
    through_folders(dom, parent, |id| dom.is_a(id, "GuiObject"))
}
