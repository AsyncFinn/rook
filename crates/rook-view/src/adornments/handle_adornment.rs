//! The HandleAdornments drawn as one piece (Box, Sphere, Cylinder, Cone,
//! Line and Image): a shape placed by its `CFrame` in its Adornee's axes,
//! and how `Shading` and `AlwaysOnTop` draw any HandleAdornment's pieces.

use rook_types::PartShape;

use super::*;

/// Pixels a LineHandleAdornment is across per unit of `Thickness`
/// (measured: 20 drew 7 pixels, 10 drew 3.5 and 4 drew 1.4, the same 50
/// and 150 studs away and at fields of view of 8° and 30°, in a viewport
/// 773 pixels high).
const LINE_PIXELS: f32 = 0.35;

/// An ImageHandleAdornment's default `Color3`, (242, 243, 243) (observed).
const IMAGE_COLOR: [f32; 3] = [242.0 / 255.0, 243.0 / 255.0, 243.0 / 255.0];

/// One HandleAdornment on a part of `size`.
///
/// Measured in Studio 0.742 (2026-10-20): the shape sits at `CFrame` from
/// the part's center, shifted by `SizeRelativeOffset` × half the part's
/// size; a cylinder runs along the CFrame's Z, `Height` long and centered,
/// with a hole of `InnerRadius`; a cone's base is at the CFrame, its tip
/// `Height` along the LookVector; a line runs `Length` along the
/// LookVector from the CFrame, a few pixels across whatever its distance;
/// an image of `Size` faces along the LookVector, turned with the CFrame
/// and seen only from in front, tinted by `Color3`.
pub(super) fn handle_adornment(
    dom: &Dom,
    id: InstanceId,
    size: Vec3,
    cx: &mut Context<'_>,
) -> Vec<(PartInstance, Pose)> {
    let float = |name, default| dom.float_property(id, name).unwrap_or(default);
    let (radius, height) = (float("Radius", 1.0), float("Height", 1.0));
    let plain = |shape| piece(dom, id, shape);
    let (piece, center) = match dom.class_name(id) {
        "BoxHandleAdornment" => (
            PartInstance {
                size: dom.vector3_property(id, "Size").unwrap_or(Vec3::ONE),
                ..plain(DrawShape::Part(PartShape::Block))
            },
            Vec3::ZERO,
        ),
        "SphereHandleAdornment" => (
            PartInstance {
                size: Vec3::splat(2.0 * radius),
                ..plain(DrawShape::Part(PartShape::Ball))
            },
            Vec3::ZERO,
        ),
        "CylinderHandleAdornment" => (
            PartInstance {
                size: Vec3::new(2.0 * radius, 2.0 * radius, height),
                ..plain(DrawShape::Adorn(AdornShape::Tube {
                    hole: AdornShape::hole(float("InnerRadius", 0.0), radius),
                }))
            },
            Vec3::ZERO,
        ),
        "ConeHandleAdornment" => (
            PartInstance {
                size: Vec3::new(2.0 * radius, 2.0 * radius, height),
                ..plain(DrawShape::Adorn(AdornShape::Cone))
            },
            Vec3::new(0.0, 0.0, -height * 0.5),
        ),
        "LineHandleAdornment" => {
            let length = float("Length", 5.0);
            let line = line(
                plain(DrawShape::Part(PartShape::Block)),
                float("Thickness", 1.0),
            );
            if length <= 0.0 {
                return Vec::new();
            }
            (
                PartInstance {
                    size: Vec3::new(1.0, 1.0, length),
                    ..line
                },
                Vec3::new(0.0, 0.0, -length * 0.5),
            )
        }
        "ImageHandleAdornment" => {
            let url = props::content_url(dom, id, &["Image"]);
            // Drawn once its image has loaded; one that can't load draws
            // its color (Inference).
            let Some(texture) = (*cx.image)(&url) else {
                return Vec::new();
            };
            let quad = dom.vector2_property(id, "Size").map_or(Vec2::ONE, |v| v.0);
            (
                PartInstance {
                    size: quad.extend(1.0),
                    color: dom
                        .color3_property(id, "Color3")
                        .map_or(IMAGE_COLOR, Color3::to_array),
                    texture,
                    ..plain(DrawShape::Adorn(AdornShape::Quad))
                },
                Vec3::ZERO,
            )
        }
        _ => return Vec::new(),
    };
    let cf = dom.cframe_property(id, "CFrame").unwrap_or_default();
    let pose = Pose {
        offset: shift(dom, id, size) + cf.position + cf.rotation * center,
        turn: cf.rotation,
    };
    shaded(dom, id, piece, pose)
}

/// A HandleAdornment's own shift: `SizeRelativeOffset` × half its part's
/// `size`.
pub(super) fn shift(dom: &Dom, id: InstanceId, size: Vec3) -> Vec3 {
    dom.vector3_property(id, "SizeRelativeOffset")
        .unwrap_or(Vec3::ZERO)
        * size
        * 0.5
}

/// `base` as a line `pixels` across on screen. Thinner than a pixel, it
/// draws one pixel across, faded by how much of it the line would cover.
pub(super) fn line_pixels(base: PartInstance, pixels: f32) -> PartInstance {
    let cover = pixels.clamp(0.0, 1.0);
    PartInstance {
        grows: Some(Grow::Across {
            pixels: pixels.max(1.0),
        }),
        transparency: 1.0 - (1.0 - base.transparency) * cover,
        ..base
    }
}

fn line(base: PartInstance, thickness: f32) -> PartInstance {
    line_pixels(base, thickness * LINE_PIXELS)
}

/// A HandleAdornment's `piece` at `pose`, as its `Shading` and
/// `AlwaysOnTop` draw it.
pub(super) fn shaded(
    dom: &Dom,
    id: InstanceId,
    piece: PartInstance,
    pose: Pose,
) -> Vec<(PartInstance, Pose)> {
    if piece.transparency >= 1.0 {
        return Vec::new();
    }
    // Inference: `AlwaysOnTop` follows the API reference (the MCP capture
    // leaves out what Studio draws on top); like Shading AlwaysOnTop, it
    // draws in exactly its color, past the tone curve (measured).
    let shading = dom.enum_item_property(id, "Shading").unwrap_or("Default");
    if dom.bool_property(id, "AlwaysOnTop") == Some(true) || shading == "AlwaysOnTop" {
        let top = PartInstance {
            light: Light::Exact,
            on_top: true,
            ..piece
        };
        return vec![(top, pose)];
    }
    let light = match shading {
        "Shaded" | "XRayShaded" => Light::Lit,
        _ => Light::Unlit,
    };
    let drawn = PartInstance { light, ..piece };
    let mut out = vec![(drawn, pose)];
    if matches!(shading, "XRay" | "XRayShaded") {
        // What parts in front hide shows at about half strength (measured:
        // red over gray 126, 152, 165 drew 190, 64, 75); over the visible
        // part the same color changes nothing.
        let ghost = PartInstance {
            transparency: 1.0 - (1.0 - drawn.transparency) * 0.5,
            on_top: true,
            ..drawn
        };
        out.push((ghost, pose));
    }
    out
}
