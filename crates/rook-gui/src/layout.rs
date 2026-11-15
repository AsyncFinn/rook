//! Where every GUI object goes: its size from `Size`, `SizeConstraint`,
//! `AutomaticSize` and the size constraints; its place from `Position` and
//! `AnchorPoint`, or from a UI layout (UIListLayout with its flex,
//! UIGridLayout, UITableLayout, UIPageLayout), on a ScrollingFrame's canvas
//! if it's in one; turned about its ancestors' centers by their `Rotation`
//! and scaled by UIScales (docs/behavior/gui/layout.md#layout,
//! docs/behavior/gui/modern.md).

mod automatic;
mod changes;
mod constraints;
mod grid;
mod list;
mod padding;
mod page;
mod pass;
mod scrolling;
mod table;
mod tree;

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::UDim2;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::text::{self, Fonts, Source, TextBlock};
use crate::world::{self, Eye, Hung};
use padding::Inner;
pub(crate) use pass::*;
pub(crate) use scrolling::Scroll;
pub(crate) use tree::*;

/// The top bar's height, which ScreenGuis start below unless they ignore
/// the inset (measured: 58 pixels in Studio 0.742's client, with the
/// current top bar; `GuiService:GetGuiInset()` says the same).
pub const GUI_INSET: f32 = 58.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placed {
    /// Its center on the screen.
    pub(crate) center: Vec2,
    /// Its size before UIScales, which its children's `Size` and
    /// `Position` scale against.
    pub(crate) local: Vec2,
    /// The UIScales on it and its ancestors, multiplied.
    pub(crate) scale: f32,
    /// `AbsoluteRotation`, in degrees.
    pub(crate) rotation: f32,
}

impl Placed {
    /// What children of an unparented object are placed in: nothing, at
    /// the origin (measured: an unparented Frame's `AbsoluteSize` is its
    /// `Size` offsets, at (0, 0)).
    const NOWHERE: Self = Self {
        center: Vec2::ZERO,
        local: Vec2::ZERO,
        scale: 1.0,
        rotation: 0.0,
    };

    pub(crate) fn size(&self) -> Vec2 {
        self.local * self.scale
    }

    pub(crate) fn top_left(&self) -> Vec2 {
        self.center - self.size() * 0.5
    }

    /// A point given in this object's own frame (pixels from its top left,
    /// before scaling and turning), on the screen.
    pub(crate) fn screen_point(self, local: Vec2) -> Vec2 {
        self.center + turn(self.rotation, (local - self.local * 0.5) * self.scale)
    }

    /// A point on the screen in this object's own frame: the inverse of
    /// [`Placed::screen_point`].
    pub(crate) fn local_point(self, screen: Vec2) -> Vec2 {
        turn(-self.rotation, screen - self.center) / self.scale + self.local * 0.5
    }
}

/// `v` turned clockwise on the screen by `degrees` (y points down).
pub(crate) fn turn(degrees: f32, v: Vec2) -> Vec2 {
    if degrees == 0.0 {
        return v;
    }
    let (sin, cos) = degrees.to_radians().sin_cos();
    Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Arranged {
    /// `AbsoluteContentSize`.
    pub(crate) content: Vec2,
    /// A grid's `AbsoluteCellSize` and `AbsoluteCellCount`.
    pub(crate) cells: Option<(Vec2, Vec2)>,
}

/// Every placed GUI object, what each layout reports, each UIPageLayout's
/// pages in order, each text object's text, each ScrollingFrame's canvas,
/// and every world GUI with the canvas size it was laid out at.
#[derive(Debug, Default)]
pub(crate) struct Layout {
    pub(crate) placed: FxHashMap<InstanceId, Placed>,
    pub(crate) arranged: FxHashMap<InstanceId, Arranged>,
    pub(crate) pages: FxHashMap<InstanceId, Vec<InstanceId>>,
    pub(crate) texts: FxHashMap<InstanceId, Laid>,
    pub(crate) scrolls: FxHashMap<InstanceId, Scroll>,
    pub(crate) world: std::collections::BTreeMap<InstanceId, (Hung, Vec2)>,
}

/// A text object's text, with what it was laid out from, so an unchanged
/// one is reused next time rather than shaped again.
#[derive(Debug)]
pub(crate) struct Laid {
    source: Source,
    /// The area inside the padding: its top left and size.
    area: (Vec2, Vec2),
    /// Whether a font it wanted was still on its way: its text lays out
    /// again when one arrives.
    waited: bool,
    pub(crate) block: TextBlock,
}
