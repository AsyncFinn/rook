//! UISizeConstraint and UIAspectRatioConstraint, on a GUI object or on a
//! UIGridLayout (whose cells they hold).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};

/// `size` held by `holder`'s size constraints, then its aspect ratio
/// constraints (measured: a 0.5 aspect with a 60-pixel maximum height
/// gives 30 × 60 whichever was added first). `parent` is the area the
/// object is placed in.
pub(crate) fn apply(dom: &Dom, holder: InstanceId, size: Vec2, parent: Vec2) -> Vec2 {
    let children = dom.children(holder);
    let mut size = size;
    for &c in children {
        if dom.class_name(c) == "UISizeConstraint" {
            size = clamp(dom, c, size);
        }
    }
    for &c in children {
        if dom.class_name(c) == "UIAspectRatioConstraint" {
            size = aspect(dom, c, size, parent);
        }
    }
    size
}

/// Measured: a constraint whose `MinSize` is above its `MaxSize` on
/// either axis does nothing at all.
fn clamp(dom: &Dom, c: InstanceId, size: Vec2) -> Vec2 {
    let min = dom
        .vector2_property(c, "MinSize")
        .map_or(Vec2::ZERO, |v| v.0);
    let max = dom
        .vector2_property(c, "MaxSize")
        .map_or(Vec2::INFINITY, |v| v.0);
    if min.x > max.x || min.y > max.y {
        return size;
    }
    size.clamp(min, max)
}

/// Measured: FitWithinMaxSize gives the largest size of the ratio inside
/// `size` (`DominantAxis` doesn't matter); ScaleWithParentSize keeps the
/// dominant axis and fits the result inside the parent when it's bigger.
fn aspect(dom: &Dom, c: InstanceId, size: Vec2, parent: Vec2) -> Vec2 {
    let ratio = dom.float_property(c, "AspectRatio").unwrap_or(1.0);
    if ratio.is_nan() || ratio <= 0.0 {
        return size;
    }
    if dom.enum_item_property(c, "AspectType") == Some("ScaleWithParentSize") {
        let kept = if dom.enum_item_property(c, "DominantAxis") == Some("Height") {
            Vec2::new(size.y * ratio, size.y)
        } else {
            Vec2::new(size.x, size.x / ratio)
        };
        if kept.x > parent.x || kept.y > parent.y {
            return fit(ratio, parent);
        }
        return kept;
    }
    fit(ratio, size)
}

/// The largest width / height = `ratio` inside `bounds`.
fn fit(ratio: f32, bounds: Vec2) -> Vec2 {
    if bounds.y > 0.0 && bounds.x / bounds.y > ratio {
        Vec2::new(bounds.y * ratio, bounds.y)
    } else {
        Vec2::new(bounds.x, bounds.x / ratio)
    }
}
