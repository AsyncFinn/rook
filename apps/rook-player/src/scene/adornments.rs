//! Adornments drawn on a part (docs/behavior/adornments.md), as Studio
//! 0.742 draws them: so far SelectionBox. Each kind has its own file.

mod selection_box;

use super::*;

pub(super) use selection_box::selection_boxes;

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
