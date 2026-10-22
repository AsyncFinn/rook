//! The area inside an object its children and text are placed in: less
//! its UIPadding, and an Inset or Middle border or its legacy style's
//! inset.

use glam::Vec2;
use rook_dom::{Dom, InstanceId};

use crate::style;

/// The area children are placed in, in their parent's own frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Inner {
    /// Its top left (the left and top padding).
    pub(crate) origin: Vec2,
    pub(crate) size: Vec2,
    /// The right and bottom padding.
    pub(crate) far: Vec2,
}

impl Inner {
    pub(crate) fn whole(size: Vec2) -> Self {
        Self {
            origin: Vec2::ZERO,
            size,
            far: Vec2::ZERO,
        }
    }
}

/// `id`'s area of `size` less its first UIPadding's and its border's part
/// inside it. Measured: left and right padding scale with the width inside
/// the border, top and bottom with the height (a 0.25 left padding in a
/// 120-wide frame with a 4-pixel Inset border is 28), and children's
/// `Size` and `Position` scale with what's left.
pub(crate) fn inner(dom: &Dom, id: InstanceId, size: Vec2) -> Inner {
    // Measured: a style's inset adds to an Inset border's (8 and 4 put a
    // child 12 in), though the style draws no border.
    let style = style::of(dom, id).map_or(0.0, |s| s.inset(size));
    let border = Vec2::splat(border_inside(dom, id) + style);
    let Some(p) = dom.find_first_child_of_class(id, "UIPadding") else {
        return Inner {
            origin: border,
            size: size - border * 2.0,
            far: border,
        };
    };
    let within = size - border * 2.0;
    let side = |name, length: f32| {
        dom.udim_property(p, name)
            .map_or(0.0, |u| u.resolve(length))
    };
    let origin = Vec2::new(side("PaddingLeft", within.x), side("PaddingTop", within.y)) + border;
    let far = Vec2::new(
        side("PaddingRight", within.x),
        side("PaddingBottom", within.y),
    ) + border;
    Inner {
        origin,
        size: size - origin - far,
        far,
    }
}

/// How far `id`'s border reaches into it. Measured: an Inset border's
/// width, or half a Middle one's, is taken off every side of the area its
/// children and text are placed in (a 4-pixel Inset border put a child at
/// (10, 20) at (14, 24), half its size 96 × 46 in 200 × 100).
fn border_inside(dom: &Dom, id: InstanceId) -> f32 {
    let width = || {
        dom.number_property(id, "BorderSizePixel")
            .unwrap_or(0.0)
            .max(0.0) as f32
    };
    match dom.enum_item_property(id, "BorderMode") {
        Some("Inset") => width(),
        Some("Middle") => width() * 0.5,
        _ => 0.0,
    }
}
