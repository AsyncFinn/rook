//! `AutomaticSize`: an object grows to hold its children.

use glam::Vec2;
use rook_dom::InstanceId;

use super::{Kid, Pass, arrange, arranger, gui_children, padding, position_of, resolve, visible};

/// `base` grown on `id`'s `AutomaticSize` axes to hold its visible
/// children, never shrunk (measured: `Size` is the least it gets).
/// `room` is what's left of the parent's area beyond the object's
/// `Position`.
///
/// Measured: with a UI layout, the children are sized in `room` and the
/// object grows to the layout's content. Without one, children's
/// offsets count; children sized by scale give Studio results that
/// depend on earlier layouts, so Rook sizes them against `base`
/// (Approximation, docs/behavior/gui.md#automaticsize).
pub(super) fn grow(pass: &mut Pass<'_>, id: InstanceId, base: Vec2, room: Vec2) -> Vec2 {
    let dom = pass.dom;
    let axes = match dom.enum_item_property(id, "AutomaticSize") {
        Some("X") => Vec2::X,
        Some("Y") => Vec2::Y,
        Some("XY") => Vec2::ONE,
        _ => return base,
    };
    let kids: Vec<InstanceId> = gui_children(dom, id)
        .into_iter()
        .filter(|&k| visible(dom, k))
        .collect();
    if kids.is_empty() {
        return base;
    }
    let extent = match arranger(dom, id) {
        Some(layout) => {
            let inner = padding::inner(dom, id, room.max(base));
            let laid: Vec<Kid> = kids
                .iter()
                .map(|&k| Kid {
                    id: k,
                    size: pass.size_of(k, inner.size),
                })
                .collect();
            let content = arrange(dom, layout, &laid, inner.size).arranged.content;
            inner.origin + content + inner.far
        }
        None => {
            let inner = padding::inner(dom, id, base);
            let mut extent = Vec2::ZERO;
            for &k in &kids {
                let size = pass.size_of(k, inner.size);
                let anchor = super::vec2_or(dom, k, "AnchorPoint", Vec2::ZERO);
                let far = resolve(position_of(dom, k), inner.size) - size * anchor + size;
                extent = extent.max(far);
            }
            inner.origin + extent + inner.far
        }
    };
    base + (extent - base).max(Vec2::ZERO) * axes
}
