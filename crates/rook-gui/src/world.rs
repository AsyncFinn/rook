//! World GUI: SurfaceGuis laid on a part's face, BillboardGuis facing
//! the camera, and the names and health bars over Humanoids' heads. Which are shown, their canvas sizes, and where they are each
//! frame; their contents are laid out and drawn like a ScreenGui's
//! (docs/behavior/gui/world.md).

mod billboard;
mod frames;
mod hung;
pub(crate) mod overhead;
mod surface;

use crate::*;
use glam::{Mat3, Vec2, Vec3};
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, WorldPlace};

use billboard::Billboard;
pub(crate) use hung::*;
use surface::Surface;

pub(crate) fn is_world_gui(dom: &Dom, id: InstanceId) -> bool {
    matches!(dom.class_name(id), "SurfaceGui" | "BillboardGui")
}

/// Whether `gui` is somewhere a client draws world GUI from: Workspace or
/// the local player's PlayerGui, however deep (measured: one inside a
/// ScreenGui there draws).
pub(crate) fn held_where_drawn(dom: &Dom, gui: InstanceId) -> bool {
    let in_workspace = dom
        .find_service("Workspace")
        .is_some_and(|ws| dom.is_ancestor_of(ws, gui));
    in_workspace
        || (dom.local_player())
            .and_then(|p| dom.find_first_child_of_class(p, "PlayerGui"))
            .is_some_and(|pg| dom.is_ancestor_of(pg, gui))
}

/// The world GUIs among `laid` (every world GUI laid out) that draw.
pub(crate) fn drawn<'a>(dom: &Dom, laid: impl Iterator<Item = &'a Hung>) -> Vec<Hung> {
    laid.filter(|h| h.shown(dom)).cloned().collect()
}
