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
    let axes = automatic_axes(pass.dom, id);
    if axes == Vec2::ZERO {
        return base;
    }
    grow_on(pass, id, base, room, axes)
}

/// 1 on each axis `id` grows on by `AutomaticSize`.
fn automatic_axes(dom: &rook_dom::Dom, id: InstanceId) -> Vec2 {
    match dom.enum_item_property(id, "AutomaticSize") {
        Some("X") => Vec2::X,
        Some("Y") => Vec2::Y,
        Some("XY") => Vec2::ONE,
        _ => Vec2::ZERO,
    }
}

/// [`grow`] on `axes` (1 for each axis that grows).
fn grow_on(pass: &mut Pass<'_>, id: InstanceId, base: Vec2, room: Vec2, axes: Vec2) -> Vec2 {
    let dom = pass.dom;
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
            // Approximation: across a list, the room comes from a parent
            // that doesn't grow itself (measured there); inside one that
            // does, the object's `Size` holds its children (Roblox's report
            // rows, in frames that grow, are as tall as their labels).
            // Along a list the room stays, so wrapping rows still fit.
            let parent = dom
                .parent(id)
                .map_or(Vec2::ZERO, |p| automatic_axes(dom, p));
            let held = parent * across_list(dom, layout);
            let inner = padding::inner(dom, id, room.max(base) * (Vec2::ONE - held) + base * held);
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
            // Children that grow themselves may use the room this object
            // may grow into, or their own wrapping lists would break; not
            // on axes they're sized by scale, where it would feed back.
            let area = padding::inner(dom, id, room.max(base)).size;
            let mut extent = Vec2::ZERO;
            for &k in &kids {
                let scaled = scale_axes(dom, k);
                let within = inner.size * scaled + area * (Vec2::ONE - scaled);
                let size = pass.size_within(k, inner.size, within);
                let size = size.max(scaled_content(pass, k, size, axes));
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

/// What a child sized by scale needs for its content, on the growing
/// `axes` its `Size` has scale on (zero elsewhere): it's measured as if it
/// grew there too, down through children sized the same way. Inference:
/// Roblox's menu sizes its help groups and report rows to content around
/// full-size children that list the rows, and nothing overlaps there.
fn scaled_content(pass: &mut Pass<'_>, id: InstanceId, size: Vec2, axes: Vec2) -> Vec2 {
    let on = axes * scale_axes(pass.dom, id);
    if on == Vec2::ZERO {
        return Vec2::ZERO;
    }
    let scaled = pass.dom.udim2_property(id, "Size").unwrap_or_default();
    // From the offset alone and in no more room: the scaled size or the
    // room would feed the parent's size back into itself (a page in a
    // canvas sized to its content grew every frame).
    let offset = Vec2::new(scaled.x.offset as f32, scaled.y.offset as f32);
    let natural = size * (Vec2::ONE - on) + offset * on;
    grow_on(pass, id, natural, natural, on) * on
}

/// 1 on the axis across a UIListLayout's `FillDirection`; zero for other
/// layouts.
fn across_list(dom: &rook_dom::Dom, layout: InstanceId) -> Vec2 {
    if dom.class_name(layout) != "UIListLayout" {
        return Vec2::ZERO;
    }
    match dom.enum_item_property(layout, "FillDirection") {
        Some("Horizontal") => Vec2::Y,
        _ => Vec2::X,
    }
}

/// 1 on each axis `id`'s `Size` has scale on.
fn scale_axes(dom: &rook_dom::Dom, id: InstanceId) -> Vec2 {
    let size = dom.udim2_property(id, "Size").unwrap_or_default();
    Vec2::new(
        f32::from(u8::from(size.x.scale != 0.0)),
        f32::from(u8::from(size.y.scale != 0.0)),
    )
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
