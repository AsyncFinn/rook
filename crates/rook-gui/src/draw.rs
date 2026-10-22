//! What the local player's ScreenGuis draw, in order: each visible GUI
//! object's background and border, a ScrollingFrame's bars, then its image
//! or text, and any Hint or Message over it all; and what each world GUI's
//! canvas draws, the same way. Order and clipping rules:
//! docs/behavior/gui/layout.md#drawing.

mod caret;
mod frame;
mod image;
mod messages;
mod scrolling;
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
    // bar.
    for q in draw.out.iter_mut() {
        q.center.y += GUI_INSET;
        q.clip[1] += GUI_INSET;
        q.clip[3] += GUI_INSET;
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
        for s in order::stack(self.dom, self.layout, root, clip) {
            self.emit(&s);
        }
    }

    fn emit(&mut self, d: &Stacked) {
        if !touches(d.placed, self.canvas) {
            return;
        }
        let (dom, out) = (self.dom, &mut *self.out);
        frame::emit(dom, d.id, d.placed, d.clip, out);
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
            text::emit(&laid.block, d.placed, d.clip, self.fields, text, out);
            if focused {
                caret::caret(dom, d.id, &laid.block, d.placed, d.clip, out);
            }
        }
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
