//! UIPadding: the area inside an object its children are placed in.

use glam::Vec2;
use rook_dom::{Dom, InstanceId};

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

/// `id`'s area of `size` less its first UIPadding's. Measured: left and
/// right scale with the width, top and bottom with the height, and
/// children's `Size` and `Position` scale with what's left.
pub(crate) fn inner(dom: &Dom, id: InstanceId, size: Vec2) -> Inner {
    let Some(p) = dom.find_first_child_of_class(id, "UIPadding") else {
        return Inner::whole(size);
    };
    let side = |name, length: f32| {
        dom.udim_property(p, name)
            .map_or(0.0, |u| u.resolve(length))
    };
    let origin = Vec2::new(side("PaddingLeft", size.x), side("PaddingTop", size.y));
    let far = Vec2::new(side("PaddingRight", size.x), side("PaddingBottom", size.y));
    Inner {
        origin,
        size: size - origin - far,
        far,
    }
}
