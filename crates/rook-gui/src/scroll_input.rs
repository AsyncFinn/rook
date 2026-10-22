//! ScrollingFrames moved by the mouse: the wheel, eased over a few
//! frames, and dragging a bar's thumb
//! (docs/behavior/gui/input.md#the-wheel).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};

use crate::Gui;
use crate::layout::Scroll;

/// Measured: one notch moves a canvas 100 pixels.
const NOTCH: f32 = 100.0;

/// Measured: an eased scroll leaves e^(-25 t) of the way to go after t
/// seconds (97.7 of a 100-pixel notch after 0.15 s): this much per 60 Hz
/// frame. Inference: it lands once less than a pixel is left.
const EASE_LEFT: f32 = 0.659;

/// A wheel scroll on its way to `target`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Eased {
    target: Vec2,
    /// What it last set `CanvasPosition` to: a script moving the canvas
    /// since stops it.
    written: Vec2,
}

/// A scroll bar's thumb held by the mouse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarDrag {
    pub frame: InstanceId,
    /// 0 for the horizontal bar, 1 for the vertical.
    axis: usize,
    from_mouse: f32,
    from_position: Vec2,
    /// Canvas pixels per pixel the mouse moves.
    rate: f32,
}

/// The axis the wheel moves `scroll` along: down where its vertical bar
/// shows, else across where its horizontal bar does (measured: a canvas
/// only wider scrolls sideways, though the bar under it leaves the window
/// a little short of the frame).
fn wheel_axis(scroll: &Scroll) -> Option<usize> {
    let reach = scroll.reach();
    let y = scroll.vertical && reach.y > 0.0;
    let x = scroll.horizontal && reach.x > 0.0;
    match (y, x) {
        (true, _) => Some(1),
        (false, true) => Some(0),
        _ => None,
    }
}

impl Gui {
    /// Scroll by `notches` of the wheel (positive is forward, up the
    /// canvas) the innermost ScrollingFrame among `objects` (top first,
    /// as [`Gui::objects_at`] gives them) that can still move that way.
    /// Returns it.
    pub fn wheel(&mut self, dom: &Dom, objects: &[InstanceId], notches: f32) -> Option<InstanceId> {
        for &id in objects {
            let Some(scroll) = self.layout.scrolls.get(&id) else {
                continue;
            };
            if dom.bool_property(id, "ScrollingEnabled") == Some(false) {
                continue;
            }
            let Some(axis) = wheel_axis(scroll) else {
                continue;
            };
            // Measured: a frame still easing towards its end takes the next
            // notch, though it can't go further.
            let reach = scroll.reach()[axis];
            let now = scroll.position[axis];
            let blocked = if notches > 0.0 {
                now <= 0.0
            } else {
                now >= reach
            };
            if blocked {
                continue;
            }
            let from = self
                .scrolling
                .get(&id)
                .map_or(scroll.position, |e| e.target);
            let mut target = from;
            target[axis] = (from[axis] - notches * NOTCH).clamp(0.0, reach);
            let written = self
                .scrolling
                .get(&id)
                .map_or(scroll.position, |e| e.written);
            self.scrolling.insert(id, Eased { target, written });
            return Some(id);
        }
        None
    }

    /// Move each wheel scroll on a frame. Returns the `CanvasPosition`s
    /// to set, for the caller to set as a script would (firing Changed).
    pub fn ease_scrolls(&mut self, dom: &Dom) -> Vec<(InstanceId, Vec2)> {
        let mut out = Vec::new();
        self.scrolling.retain(|&id, eased| {
            let Some(now) = dom.vector2_property(id, "CanvasPosition").map(|v| v.0) else {
                return false;
            };
            if now != eased.written {
                return false;
            }
            let left = (eased.target - now) * EASE_LEFT;
            let next = if left.abs().max_element() < 1.0 {
                eased.target
            } else {
                eased.target - left
            };
            eased.written = next;
            out.push((id, next));
            next != eased.target
        });
        out
    }

    /// Take hold of `frame`'s scroll bar thumb if `at` (GUI coordinates)
    /// is on it.
    pub fn grab_bar(&self, frame: InstanceId, at: Vec2) -> Option<BarDrag> {
        let scroll = self.layout.scrolls.get(&frame)?;
        let placed = self.layout.placed.get(&frame)?;
        let local = placed.local_point(at);
        let axis = scroll.thumb_at(local)?;
        // Measured: the canvas moves canvas ÷ window pixels per pixel.
        let rate = scroll.canvas[axis] / scroll.window[axis].max(1.0);
        Some(BarDrag {
            frame,
            axis,
            from_mouse: local[axis],
            from_position: scroll.position,
            rate,
        })
    }

    /// Where a dragged bar puts its canvas with the mouse at `at`, kept
    /// within the canvas.
    pub fn drag_bar(&self, drag: &BarDrag, at: Vec2) -> Option<Vec2> {
        let scroll = self.layout.scrolls.get(&drag.frame)?;
        let placed = self.layout.placed.get(&drag.frame)?;
        let moved = placed.local_point(at)[drag.axis] - drag.from_mouse;
        let mut position = drag.from_position;
        position[drag.axis] =
            (position[drag.axis] + moved * drag.rate).clamp(0.0, scroll.reach()[drag.axis]);
        Some(position)
    }
}
