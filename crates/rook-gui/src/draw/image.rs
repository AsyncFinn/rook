//! An ImageLabel's or ImageButton's image, over its background, laid out
//! by its `ScaleType`: stretched, fitted, cropped, tiled or sliced in nine
//! (docs/behavior/gui/images.md).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::{GuiQuad, ImagePaint, Paint};

use crate::images::{GuiImage, Images};
use crate::layout::{Placed, resolve, vec2_or};

/// Part of the image drawn over part of the object: where, in the
/// object's own pixels, and from where, in the image's (or, tiled, in
/// tiles' worth of them past its edges).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Piece {
    pub(super) to: [Vec2; 2],
    pub(super) from: [Vec2; 2],
}

/// How an image is drawn: its file, its color, and how it's sampled.
#[derive(Debug, Clone, Copy)]
pub(super) struct Look {
    pub(super) image: GuiImage,
    pub(super) color: [f32; 4],
    pub(super) pixelated: bool,
    pub(super) tiled: bool,
}

pub(super) fn emit(
    dom: &Dom,
    id: InstanceId,
    p: Placed,
    clip: [f32; 4],
    images: &mut Images,
    out: &mut Vec<GuiQuad>,
) {
    let url = shown_image(dom, id);
    let Some(image) = images.get(&url) else {
        return;
    };
    let alpha = 1.0 - dom.float_property(id, "ImageTransparency").unwrap_or(0.0);
    if alpha <= 0.0 {
        return;
    }
    let rgb = dom
        .color3_property(id, "ImageColor3")
        .map_or([1.0; 3], |c| [c.r, c.g, c.b]);
    let rect = source_rect(dom, id, image);
    let (pieces, tiled) = pieces(dom, id, p.local, rect);
    let look = Look {
        image,
        color: [rgb[0], rgb[1], rgb[2], alpha],
        pixelated: dom.enum_item_property(id, "ResampleMode") == Some("Pixelated"),
        tiled,
    };
    out.extend(
        pieces
            .iter()
            .filter_map(|piece| quad(p, *piece, look, clip)),
    );
}

/// The image an object shows now: an ImageButton's `HoverImage` while
/// hovered and `PressedImage` while pressed, when set (Inference: a
/// pressed button without one shows its `Image`).
pub(crate) fn shown_image(dom: &Dom, id: InstanceId) -> String {
    let state = match dom.enum_item_property(id, "GuiState") {
        Some("Hover") => Some("HoverImageContent"),
        Some("Press") => Some("PressedImageContent"),
        _ => None,
    };
    state
        .filter(|_| dom.class_name(id) == "ImageButton")
        .and_then(|name| dom.content_property(id, name))
        .filter(|url| !url.is_empty())
        .or_else(|| dom.content_property(id, "ImageContent"))
        .unwrap_or_default()
}

/// Every image an object may show: its `Image`, and an ImageButton's
/// hover and pressed images.
pub(crate) fn all_images(dom: &Dom, id: InstanceId) -> impl Iterator<Item = String> + '_ {
    ["ImageContent", "HoverImageContent", "PressedImageContent"]
        .into_iter()
        .filter_map(move |name| dom.content_property(id, name))
        .filter(|url| !url.is_empty())
}

/// `piece` on the screen, or nothing when it covers no area.
pub(super) fn quad(p: Placed, piece: Piece, look: Look, clip: [f32; 4]) -> Option<GuiQuad> {
    let [lo, hi] = piece.to;
    let size = (hi - lo) * p.scale;
    if size.x <= 0.0 || size.y <= 0.0 {
        return None;
    }
    let texels = Vec2::new(look.image.width as f32, look.image.height as f32);
    let (a, b) = (piece.from[0] / texels, piece.from[1] / texels);
    Some(GuiQuad {
        center: p.screen_point((lo + hi) * 0.5),
        size,
        rotation: p.rotation.to_radians(),
        color: look.color,
        clip,
        paint: Paint::Image(ImagePaint {
            hash: look.image.hash,
            uv: [a.x, a.y, b.x, b.y],
            pixelated: look.pixelated,
            tiled: look.tiled,
        }),
    })
}

/// The part of the image shown, corner to corner in its pixels:
/// `ImageRectOffset` to `ImageRectOffset + ImageRectSize`, or all of it
/// when the size is (0, 0). Measured: a size past the image is cut at its
/// edges, a negative size mirrors it, and a size of 0 on one axis shows a
/// line of pixels stretched.
fn source_rect(dom: &Dom, id: InstanceId, image: GuiImage) -> [Vec2; 2] {
    let texels = Vec2::new(image.width as f32, image.height as f32);
    let size = vec2_or(dom, id, "ImageRectSize", Vec2::ZERO);
    if size == Vec2::ZERO {
        return [Vec2::ZERO, texels];
    }
    let offset = vec2_or(dom, id, "ImageRectOffset", Vec2::ZERO);
    [offset, offset + size].map(|c| c.clamp(Vec2::ZERO, texels))
}

/// What `ScaleType` makes of `rect` on an object `size` pixels big, and
/// whether the pieces tile.
fn pieces(dom: &Dom, id: InstanceId, size: Vec2, rect: [Vec2; 2]) -> (Vec<Piece>, bool) {
    let whole = [Vec2::ZERO, size];
    let span = (rect[1] - rect[0]).abs();
    let stretch = Piece {
        to: whole,
        from: rect,
    };
    let pieces = match dom.enum_item_property(id, "ScaleType") {
        Some("Fit") if span.x > 0.0 && span.y > 0.0 => {
            let shown = span * (size / span).min_element();
            let lo = (size - shown) * 0.5;
            vec![Piece {
                to: [lo, lo + shown],
                from: rect,
            }]
        }
        Some("Crop") if span.x > 0.0 && span.y > 0.0 => {
            // The share of the rect that fits, about its middle.
            let kept = size / (span * (size / span).max_element());
            let middle = (rect[0] + rect[1]) * 0.5;
            let half = (rect[1] - rect[0]) * kept * 0.5;
            vec![Piece {
                to: whole,
                from: [middle - half, middle + half],
            }]
        }
        // Measured: an ImageRect turns tiling off; a TileSize of 0 draws
        // the image once.
        Some("Tile")
            if dom
                .vector2_property(id, "ImageRectSize")
                .is_none_or(|v| v.0 == Vec2::ZERO) =>
        {
            let tile_size = dom.udim2_property(id, "TileSize").unwrap_or_default();
            let tile = resolve(tile_size, size);
            let tile = Vec2::select(tile.cmpgt(Vec2::ZERO), tile, size);
            let tiles = size / tile;
            return (
                vec![Piece {
                    to: whole,
                    from: [Vec2::ZERO, rect[1] * tiles],
                }],
                true,
            );
        }
        Some("Slice") => slices(dom, id, size, rect),
        _ => vec![stretch],
    };
    (pieces, false)
}

/// Nine pieces: the corners at their size times `SliceScale`, the edges
/// stretched along them and the middle both ways (docs/behavior/gui/images.md#slice).
fn slices(dom: &Dom, id: InstanceId, size: Vec2, rect: [Vec2; 2]) -> Vec<Piece> {
    let span = (rect[1] - rect[0]).abs();
    let (a, b) = dom
        .rect_property(id, "SliceCenter")
        .map_or((Vec2::ZERO, Vec2::ZERO), |r| {
            (Vec2::new(r.min.x, r.min.y), Vec2::new(r.max.x, r.max.y))
        });
    // Measured: corners given the wrong way round are swapped, and the
    // center counts from the rect's corner.
    let lo = a.min(b).clamp(Vec2::ZERO, span);
    let hi = a.max(b).clamp(lo, span);
    let scale = dom.float_property(id, "SliceScale").unwrap_or(1.0);
    let at = |v: Vec2| rect[0] + (rect[1] - rect[0]) * v / span.max(Vec2::splat(f32::MIN_POSITIVE));
    if scale <= 0.0 {
        // Observed: a SliceScale of 0 fills the object with the
        // bottom right corner.
        return vec![Piece {
            to: [Vec2::ZERO, size],
            from: [at(hi), at(span)],
        }];
    }
    let mut near = lo * scale;
    let mut far = (span - hi) * scale;
    // Measured: corners that don't fit shrink together, keeping their
    // shape, until they do on both axes.
    let fit = (size / (near + far)).min_element();
    if fit < 1.0 {
        near *= fit;
        far *= fit;
    }
    let to = [Vec2::ZERO, near, size - far, size];
    let from = [Vec2::ZERO, lo, hi, span];
    let mut out = Vec::with_capacity(9);
    for row in 0..3 {
        for col in 0..3 {
            let pick =
                |points: &[Vec2; 4], i: usize| Vec2::new(points[i + col].x, points[i + row].y);
            out.push(Piece {
                to: [pick(&to, 0), pick(&to, 1)],
                from: [at(pick(&from, 0)), at(pick(&from, 1))],
            });
        }
    }
    out
}
