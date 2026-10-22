//! `AutomaticSize`: an object grows to hold its children.

use glam::Vec2;
use rook_dom::InstanceId;

use crate::text::{self, Source};

use super::{Kid, Pass, arranger, gui_children, padding, position_of, resolve, visible};

/// `base` grown on `id`'s `AutomaticSize` axes to hold its visible
/// children, never shrunk (measured: `Size` is the least it gets).
/// `room` is what's left of the parent's area beyond the object's
/// `Position`.
///
/// Measured: with a UI layout, the children are sized in `room` and the
/// object grows to the layout's content. Without one, children's
/// offsets count; children sized by scale give Studio results that
/// depend on earlier layouts, so Rook sizes them against `base`
/// (Approximation, docs/behavior/gui/layout.md#automaticsize).
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
    let text = text_extent(pass, id, base, axes);
    if kids.is_empty() {
        return base + (text - base).max(Vec2::ZERO) * axes;
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
            let content = pass.arrange(layout, &laid, inner.size).arranged.content;
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
    let extent = extent.max(text);
    base + (extent - base).max(Vec2::ZERO) * axes
}

/// What a text object's text needs: its `TextBounds` inside its padding.
/// Measured: wrapped text grows on Y at the object's width; Inference:
/// growing on X lays it out unwrapped.
fn text_extent(pass: &mut Pass<'_>, id: InstanceId, base: Vec2, axes: Vec2) -> Vec2 {
    let Some(source) = Source::of(pass.dom, id) else {
        return Vec2::ZERO;
    };
    let inner = padding::inner(pass.dom, id, base);
    let wrap = (source.wrapped && axes.x == 0.0).then_some(inner.size.x);
    let bounds = text::lay_out(pass.fonts, &source, inner.size, Some(wrap)).bounds;
    inner.origin + bounds + inner.far
}
