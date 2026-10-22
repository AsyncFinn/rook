//! UIListLayout, without flex and wrapping (docs/roadmap/m8-gui.md, step 7), and the
//! sort order every UI layout uses.

use glam::Vec2;
use rook_dom::{Dom, InstanceId};

use super::{Arranged, Arrangement, Kid, alignment};

/// The order `layout` places `kids` in, as indices. Measured: ties keep
/// child order, and Name compares bytes (`Z` before `a`).
pub(super) fn sorted(dom: &Dom, layout: InstanceId, kids: &[Kid]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..kids.len()).collect();
    match dom.enum_item_property(layout, "SortOrder") {
        Some("LayoutOrder") => {
            order.sort_by_key(|&i| {
                dom.number_property(kids[i].id, "LayoutOrder")
                    .unwrap_or(0.0) as i64
            });
        }
        Some("Name") => order.sort_by(|&a, &b| dom.name(kids[a].id).cmp(dom.name(kids[b].id))),
        // Custom, deprecated, is child order.
        _ => {}
    }
    order
}

/// Measured: items follow each other along `FillDirection`, `Padding`
/// apart (its scale against the area's length that way); the whole run
/// is aligned along it, and each item on its own across it. Children's
/// `Position` and `AnchorPoint` don't count.
pub(super) fn arrange(dom: &Dom, layout: InstanceId, kids: &[Kid], inner: Vec2) -> Arrangement {
    let vertical = dom.enum_item_property(layout, "FillDirection") != Some("Horizontal");
    let pick = |v: Vec2| if vertical { (v.y, v.x) } else { (v.x, v.y) };
    let (length, width) = pick(inner);
    let (along_factor, across_factor) = pick(alignment(dom, layout));
    let padding = dom
        .udim_property(layout, "Padding")
        .map_or(0.0, |u| u.resolve(length));
    let total = kids.iter().map(|k| pick(k.size).0).sum::<f32>()
        + padding * kids.len().saturating_sub(1) as f32;
    let widest = kids.iter().map(|k| pick(k.size).1).fold(0.0, f32::max);
    let mut at = (length - total) * along_factor;
    let slots = kids
        .iter()
        .map(|k| {
            let (along, across) = pick(k.size);
            let slot = (at, (width - across) * across_factor);
            at += along + padding;
            if vertical {
                Vec2::new(slot.1, slot.0)
            } else {
                Vec2::new(slot.0, slot.1)
            }
        })
        .collect();
    let content = if vertical {
        Vec2::new(widest, total)
    } else {
        Vec2::new(total, widest)
    };
    Arrangement {
        slots,
        arranged: Arranged {
            content,
            cells: None,
        },
        cell: None,
    }
}
