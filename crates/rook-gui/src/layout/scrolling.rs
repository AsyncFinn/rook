//! ScrollingFrame: the canvas its children are placed in, the window it's
//! seen through, which scroll bars show and where the canvas sits
//! (docs/behavior/gui/scrolling.md).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};

use super::{Pass, Placed, arranger, gui_children, padding, resolve, visible};

/// A ScrollingFrame as laid out, in its own pixels (before UIScales).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Scroll {
    /// `AbsoluteCanvasSize`.
    pub(crate) canvas: Vec2,
    /// `AbsoluteWindowSize`, and where the window's top left is in the
    /// frame.
    pub(crate) window: Vec2,
    pub(crate) window_at: Vec2,
    /// `CanvasPosition`, kept within the canvas.
    pub(crate) position: Vec2,
    /// Whether the vertical and the horizontal bar show.
    pub(crate) vertical: bool,
    pub(crate) horizontal: bool,
    pub(crate) thickness: f32,
    /// `BorderSizePixel`: a line this thick parts each bar from the
    /// window.
    pub(crate) border: f32,
    /// The vertical bar is on the left.
    pub(crate) left: bool,
    /// Where the canvas's top left is in the frame before it scrolls.
    shift: Vec2,
}

impl Scroll {
    /// Where the canvas's top left is in the frame: measured, children
    /// land on whole pixels (a `CanvasPosition` of 10.6 moved them 11).
    pub(crate) fn origin(&self) -> Vec2 {
        self.shift - self.position.round()
    }

    /// How far the canvas can scroll.
    pub(crate) fn reach(&self) -> Vec2 {
        (self.canvas - self.window).max(Vec2::ZERO)
    }
}

/// Lay out a ScrollingFrame placed `at` (its `CanvasSize` scales with
/// `area`, its parent's) and what its canvas holds.
pub(super) fn lay_out(pass: &mut Pass<'_>, id: InstanceId, at: Placed, area: Vec2) {
    let dom = pass.dom;
    let settings = Settings::read(dom, id);
    let wanted = resolve(
        dom.udim2_property(id, "CanvasSize").unwrap_or_default(),
        area,
    );
    let wanted = wanted.max(Vec2::ZERO);
    let kids = gui_children(dom, id);
    let mut scroll = settings.scroll(at.local, wanted);
    place(pass, id, at, &kids, &scroll);
    if settings.automatic != Vec2::ZERO {
        // Measured: the canvas grows to what its visible descendants
        // reach, never below `CanvasSize`.
        let content =
            wanted + (reached(pass, id, at, &scroll) - wanted).max(Vec2::ZERO) * settings.automatic;
        let grown = settings.scroll(at.local, content);
        if grown != scroll {
            scroll = grown;
            place(pass, id, at, &kids, &scroll);
        }
    }
    pass.out.scrolls.insert(id, scroll);
}

/// Place `kids` on `scroll`'s canvas.
fn place(pass: &mut Pass<'_>, id: InstanceId, at: Placed, kids: &[InstanceId], scroll: &Scroll) {
    if kids.is_empty() {
        return;
    }
    let dom = pass.dom;
    let mut inner = padding::inner(dom, id, scroll.canvas);
    inner.origin += scroll.origin();
    pass.place_children(at, kids, inner, arranger(dom, id));
}

/// The far corner of what `id`'s visible descendants cover, from its
/// canvas's top left, with the canvas's right and bottom padding.
/// Inference: a ScrollingFrame inside counts with its own rectangle, not
/// its canvas.
fn reached(pass: &Pass<'_>, id: InstanceId, at: Placed, scroll: &Scroll) -> Vec2 {
    let dom = pass.dom;
    let origin = scroll.origin();
    let mut far = Vec2::ZERO;
    let mut stack = gui_children(dom, id);
    while let Some(d) = stack.pop() {
        if !visible(dom, d) {
            continue;
        }
        let Some(p) = pass.out.placed.get(&d) else {
            continue;
        };
        let corner = at.local_point(p.center) + p.size() / at.scale * 0.5 - origin;
        far = far.max(corner);
        if dom.class_name(d) != "ScrollingFrame" {
            stack.extend(gui_children(dom, d));
        }
    }
    far + padding::inner(dom, id, scroll.canvas).far
}

/// What decides a ScrollingFrame's window and bars.
struct Settings {
    thickness: f32,
    border: f32,
    /// Which bars `ScrollingDirection` allows: vertical, horizontal.
    allowed: [bool; 2],
    /// `VerticalScrollBarInset` and `HorizontalScrollBarInset`.
    insets: [Option<&'static str>; 2],
    left: bool,
    /// The `AutomaticCanvasSize` axes.
    automatic: Vec2,
    /// The `CanvasPosition` asked for.
    requested: Vec2,
}

impl Settings {
    fn read(dom: &Dom, id: InstanceId) -> Self {
        let number = |name| dom.number_property(id, name).unwrap_or(0.0).max(0.0) as f32;
        let thickness = number("ScrollBarThickness");
        let border = number("BorderSizePixel");
        let direction = dom.enum_item_property(id, "ScrollingDirection");
        // Inference: a bar 0 thick doesn't show at all.
        let allowed = [
            thickness > 0.0 && direction != Some("X"),
            thickness > 0.0 && direction != Some("Y"),
        ];
        Self {
            thickness,
            border,
            allowed,
            insets: [
                dom.enum_item_property(id, "VerticalScrollBarInset"),
                dom.enum_item_property(id, "HorizontalScrollBarInset"),
            ],
            left: dom.enum_item_property(id, "VerticalScrollBarPosition") == Some("Left"),
            automatic: match dom.enum_item_property(id, "AutomaticCanvasSize") {
                Some("X") => Vec2::X,
                Some("Y") => Vec2::Y,
                Some("XY") => Vec2::ONE,
                _ => Vec2::ZERO,
            },
            requested: dom
                .vector2_property(id, "CanvasPosition")
                .map_or(Vec2::ZERO, |v| v.0),
        }
    }

    /// The frame of `size` scrolling `content` (`CanvasSize`, grown by
    /// `AutomaticCanvasSize`).
    fn scroll(&self, size: Vec2, content: Vec2) -> Scroll {
        // Measured: a bar shows when the content is bigger than the window
        // that way, the other bar taking its room; it takes its thickness
        // and the border's width from the window.
        let room = self.thickness + self.border;
        let [can_v, can_h] = self.allowed;
        let mut vertical = can_v && content.y > size.y;
        let horizontal = can_h && content.x > size.x - if vertical { room } else { 0.0 };
        vertical |= can_v && content.y > size.y - if horizontal { room } else { 0.0 };
        let window = size
            - Vec2::new(
                if vertical { room } else { 0.0 },
                if horizontal { room } else { 0.0 },
            );
        // Measured: the canvas is at least the frame, less a bar's
        // thickness where its inset is Always, or ScrollBar while it shows.
        let inset = |inset: Option<&str>, shown: bool| match inset {
            Some("Always") => self.thickness,
            Some("ScrollBar") if shown => self.thickness,
            _ => 0.0,
        };
        let inset = Vec2::new(
            inset(self.insets[0], vertical),
            inset(self.insets[1], horizontal),
        );
        let canvas = content.max(size - inset);
        let left = self.left && vertical;
        let shift = Vec2::new(if self.left { inset.x } else { 0.0 }, 0.0);
        let mut scroll = Scroll {
            canvas,
            window,
            window_at: Vec2::new(if left { room } else { 0.0 }, 0.0),
            position: Vec2::ZERO,
            vertical,
            horizontal,
            thickness: self.thickness,
            border: self.border,
            left,
            shift,
        };
        scroll.position = self.requested.clamp(Vec2::ZERO, scroll.reach());
        scroll
    }
}
