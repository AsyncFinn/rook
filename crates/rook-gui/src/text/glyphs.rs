//! Glyphs rasterized for drawing: skrifa's outlines filled by zeno into
//! coverage masks, kept per face, glyph and size.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rook_types::GlyphMask;
use rustc_hash::FxHashMap;
use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use zeno::{Command, Mask, Point};

use super::fonts::{FaceId, Fonts};

/// A glyph's mask and where it sits from the pen on the baseline, in
/// pixels (y down).
#[derive(Debug, Clone)]
pub(crate) struct Raster {
    pub(crate) mask: Arc<GlyphMask>,
    pub(crate) left: f32,
    pub(crate) top: f32,
}

/// Mask keys are unique across every DataModel, since one renderer may
/// draw several over its life.
static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

/// Sizes are kept to a quarter pixel.
const SIZE_STEPS: f32 = 4.0;

#[derive(Debug, Default)]
pub(crate) struct Glyphs {
    cache: FxHashMap<(FaceId, u32, u32), Option<Raster>>,
}

impl Glyphs {
    /// Glyph `id` of `face` drawn with a line height of `px` pixels;
    /// `None` for one with no outline (a space).
    pub(crate) fn get(&mut self, fonts: &Fonts, face: FaceId, id: u32, px: f32) -> Option<Raster> {
        let step = (px * SIZE_STEPS).round().max(1.0) as u32;
        self.cache
            .entry((face, id, step))
            .or_insert_with(|| rasterize(fonts, face, id, step as f32 / SIZE_STEPS))
            .clone()
    }
}

fn rasterize(fonts: &Fonts, face: FaceId, id: u32, px: f32) -> Option<Raster> {
    let f = fonts.face(face);
    let font = f.font();
    let outline = font.outline_glyphs().get(id.into())?;
    let em = px * f.upm / f.line;
    let mut pen = Pen(Vec::new());
    outline
        .draw(
            DrawSettings::unhinted(Size::new(em), LocationRef::default()),
            &mut pen,
        )
        .ok()?;
    if pen.0.is_empty() {
        return None;
    }
    let (coverage, placement) = Mask::new(pen.0.as_slice()).render();
    if placement.width == 0 || placement.height == 0 {
        return None;
    }
    Some(Raster {
        mask: Arc::new(GlyphMask {
            key: NEXT_KEY.fetch_add(1, Ordering::Relaxed),
            width: placement.width,
            height: placement.height,
            coverage,
        }),
        left: placement.left as f32,
        top: placement.top as f32,
    })
}

/// Outline commands with y turned to point down.
struct Pen(Vec<Command>);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(Command::MoveTo(Point::new(x, -y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(Command::LineTo(Point::new(x, -y)));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0
            .push(Command::QuadTo(Point::new(cx0, -cy0), Point::new(x, -y)));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(Command::CurveTo(
            Point::new(cx0, -cy0),
            Point::new(cx1, -cy1),
            Point::new(x, -y),
        ));
    }

    fn close(&mut self) {
        self.0.push(Command::Close);
    }
}
