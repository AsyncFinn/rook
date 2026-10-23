//! What the local player's ScreenGuis draw, in order: each visible GUI
//! object's background and border (or legacy style), a ScrollingFrame's
//! bars, then its image or text, cut by its UICorner and colored by its
//! UIGradient, then its UIStroke; CanvasGroups drawn apart and laid over
//! as one; and any Hint or Message over it all; and what each world GUI's
//! canvas draws, the same way. Order and clipping rules:
//! docs/behavior/gui/layout.md#drawing, docs/behavior/gui/modern.md.

mod caret;
mod corner;
mod frame;
mod gradient;
mod group;
mod image;
mod messages;
mod scrolling;
mod stroke;
mod style;
mod text;

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::{GuiQuad, WorldCanvas};

use crate::images::Images;
use crate::layout::{GUI_INSET, Layout, Placed, turn};
use crate::order::{self, Clip, Stacked};

/// The fonts and glyph masks text is drawn with.
pub(crate) struct Text<'a> {
    pub(crate) fonts: &'a mut crate::text::Fonts,
    pub(crate) glyphs: &'a mut crate::text::Glyphs,
}

pub(crate) use image::all_images;

/// Whether `id` draws an image.
pub(crate) fn is_image(dom: &Dom, id: InstanceId) -> bool {
    matches!(dom.class_name(id), "ImageLabel" | "ImageButton")
}

/// Bottom first.
pub(crate) fn quads(
    dom: &Dom,
    layout: &Layout,
    text: &mut Text<'_>,
    images: &mut Images,
    focus: Option<InstanceId>,
) -> Vec<GuiQuad> {
    let mut out = Vec::new();
    let mut draw = Draw {
        dom,
        layout,
        text,
        images,
        out: &mut out,
        fields: false,
        focus,
        canvas: GuiQuad::NO_CLIP,
    };
    for screen in order::screens(dom) {
        draw.collector(screen, Clip::NONE);
    }
    // Laid out in GUI coordinates, drawn in the window's, below the top
    // bar: the quads, what clips them, and the corners and gradients they
    // carry, which are placed in the same pixels.
    for q in draw.out.iter_mut() {
        q.center.y += GUI_INSET;
        q.clip[1] += GUI_INSET;
        q.clip[3] += GUI_INSET;
        if let Some(shape) = &mut q.shape {
            shape.center.y += GUI_INSET;
        }
        if let Some(gradient) = &mut q.gradient {
            gradient.origin.y += GUI_INSET;
        }
    }
    messages::emit(dom, draw.text, draw.out);
    out
}

/// What each of `guis` (world GUIs, laid out) draws on its canvas. Its
/// text is drawn from distance fields, so it stays sharp however near it's
/// seen.
pub(crate) fn world(
    dom: &Dom,
    layout: &Layout,
    guis: &[InstanceId],
    text: &mut Text<'_>,
    images: &mut Images,
    focus: Option<InstanceId>,
) -> Vec<WorldCanvas> {
    let mut canvases = Vec::new();
    for &gui in guis {
        let Some(placed) = layout.placed.get(&gui) else {
            continue;
        };
        let size = placed.local;
        let clip = order::canvas_clip(dom, gui, size);
        let mut quads = Vec::new();
        Draw {
            dom,
            layout,
            text,
            images: &mut *images,
            out: &mut quads,
            fields: true,
            focus,
            canvas: [0.0, 0.0, size.x, size.y],
        }
        .collector(gui, clip);
        canvases.push(WorldCanvas {
            quads,
            size,
            light_influence: dom.float_property(gui, "LightInfluence").unwrap_or(0.0),
            brightness: dom.float_property(gui, "Brightness").unwrap_or(1.0),
            always_on_top: dom.bool_property(gui, "AlwaysOnTop") == Some(true),
            max_distance: order::max_distance(dom, gui),
        });
    }
    canvases
}

struct Draw<'a, 'b> {
    dom: &'a Dom,
    layout: &'a Layout,
    text: &'a mut Text<'b>,
    images: &'a mut Images,
    out: &'a mut Vec<GuiQuad>,
    /// Text is drawn from distance fields at every size (world GUI).
    fields: bool,
    /// The focused TextBox, which shows its caret.
    focus: Option<InstanceId>,
    /// Objects wholly outside this rectangle aren't drawn (observed: on a
    /// SurfaceGui, frames spilling past the canvas drew, ones past it
    /// didn't); their contents still may be.
    canvas: [f32; 4],
}

impl Draw<'_, '_> {
    /// What a LayerCollector holds, in its `ZIndexBehavior`'s order.
    fn collector(&mut self, root: InstanceId, clip: Clip) {
        let stack = order::stack(self.dom, self.layout, root, clip);
        self.emit_all(&stack);
    }

    /// `stack`'s objects in order, each CanvasGroup with what it holds
    /// drawn into its group.
    fn emit_all(&mut self, stack: &[Stacked]) {
        let mut taken = vec![false; stack.len()];
        for (i, d) in stack.iter().enumerate() {
            if taken[i] {
                continue;
            }
            if self.dom.class_name(d.id) != "CanvasGroup" {
                self.emit(d);
                continue;
            }
            let mut held = Vec::new();
            for (j, e) in stack.iter().enumerate().skip(i + 1) {
                if self.dom.is_ancestor_of(d.id, e.id) {
                    taken[j] = true;
                    held.push(*e);
                }
            }
            group::emit(self, d, &held);
        }
    }

    fn emit(&mut self, d: &Stacked) {
        if !touches(d.placed, self.canvas) {
            return;
        }
        let radii = self.own(d, true);
        stroke::emit(self.dom, d.id, d.placed, radii, d.clip, self.out);
    }

    /// What `d` draws of itself, cut by its UICorner and, when `graded`,
    /// colored by its UIGradient; its UICorner's radii.
    fn own(&mut self, d: &Stacked, graded: bool) -> Option<[f32; 4]> {
        let (dom, out) = (self.dom, &mut *self.out);
        let start = out.len();
        let radii = corner::radii(dom, d.id, d.placed);
        match crate::style::of(dom, d.id) {
            Some(look) => style::emit(dom, d.id, look, d.placed, d.clip, self.images, out),
            None => frame::emit(dom, d.id, (d.placed, d.clip), radii.is_some(), out),
        }
        if let Some(scroll) = self.layout.scrolls.get(&d.id) {
            scrolling::emit(dom, d.id, d.placed, scroll, d.clip, self.images, out);
        }
        if is_image(dom, d.id) {
            image::emit(dom, d.id, d.placed, d.clip, self.images, out);
        }
        if let Some(laid) = self.layout.texts.get(&d.id) {
            let focused = self.focus == Some(d.id);
            let (text, out) = (&mut *self.text, &mut *self.out);
            if focused {
                caret::selection(dom, d.id, &laid.block, d.placed, d.clip, out);
            }
            let outline = stroke::text_outline(dom, d.id, d.placed);
            let at = (d.placed, d.clip);
            text::emit(&laid.block, at, self.fields, outline, text, out);
            if focused {
                caret::caret(dom, d.id, &laid.block, d.placed, d.clip, out);
            }
        }
        let own = &mut self.out[start..];
        if let Some(radii) = radii {
            let shape = corner::shape(d.placed, radii);
            for q in own.iter_mut() {
                q.shape = Some(shape);
            }
        }
        let g = graded.then(|| gradient::of(dom, d.id, d.placed, d.placed.size()));
        if let Some(g) = g.flatten() {
            for q in own.iter_mut().filter(|q| q.gradient.is_none()) {
                q.gradient = Some(g.clone());
            }
        }
        radii
    }
}

/// Whether `p`'s rectangle, turned, reaches into `area`.
fn touches(p: Placed, area: [f32; 4]) -> bool {
    let half = p.size().abs() * 0.5;
    let corners = [half, Vec2::new(half.x, -half.y)].map(|c| turn(p.rotation, c).abs());
    let reach = corners[0].max(corners[1]);
    let (lo, hi) = (p.center - reach, p.center + reach);
    lo.x <= area[2] && lo.y <= area[3] && hi.x >= area[0] && hi.y >= area[1]
}
