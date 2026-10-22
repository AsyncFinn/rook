//! UIPageLayout: pages side by side along `FillDirection`, the current one
//! in view (docs/behavior/gui/modern.md#uipagelayout). Its methods and the
//! tween between pages are in `rook-lua` (`luau/page_layout.luau`), which
//! sets where along the pages the view is while it turns.

use glam::Vec2;
use rook_dom::{Dom, InstanceId};

use super::{Arranged, Arrangement, Kid, alignment};

/// Measured: each page has a slot as big as the area, `Padding` apart
/// along (its scale against the area's length), and sits in it by the
/// alignments; the slot at `at` (a page index, fractional while turning,
/// and past the ends after a `Circular` step) is in view. Measured: pages
/// stay in their own slots past the ends, so a step round without
/// `Animated` leaves the new page out of view. Inference: the content is
/// as wide as the area across.
pub(super) fn arrange(
    dom: &Dom,
    layout: InstanceId,
    pages: &[Kid],
    inner: Vec2,
    at: f32,
) -> Arrangement {
    let vertical = dom.enum_item_property(layout, "FillDirection") == Some("Vertical");
    let pick = |v: Vec2| if vertical { Vec2::new(v.y, v.x) } else { v };
    let area = pick(inner);
    let padding = dom
        .udim_property(layout, "Padding")
        .map_or(0.0, |u| u.resolve(area.x));
    let stride = area.x + padding;
    let share = pick(alignment(dom, layout));
    let count = pages.len() as f32;
    let slots = (pages.iter().enumerate())
        .map(|(i, k)| {
            let size = pick(k.size);
            pick(Vec2::new((i as f32 - at) * stride, 0.0) + (area - size) * share)
        })
        .collect();
    let along = (count * stride - padding).max(0.0);
    Arrangement {
        slots,
        sizes: pages.iter().map(|k| k.size).collect(),
        arranged: Arranged {
            content: pick(Vec2::new(along, area.y)),
            cells: None,
        },
    }
}
