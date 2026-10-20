//! The Box, Sphere, Cylinder and Cone HandleAdornments: a shape placed by
//! its `CFrame` in its Adornee's axes.

use rook_types::PartShape;

use super::*;

/// One HandleAdornment on a part of `size`, or nothing for the kinds not
/// drawn yet (Line, Image, Wireframe).
///
/// Measured in Studio 0.742 (2026-10-20): the shape sits at `CFrame` from
/// the part's centre, shifted by `SizeRelativeOffset` × half the part's
/// size; a cylinder runs along the CFrame's Z, `Height` long and centred,
/// with a hole of `InnerRadius`; a cone's base is at the CFrame, its tip
/// `Height` along the LookVector. `Shading` Default draws `Color3`
/// without light, Shaded lit; XRay and XRayShaded also show faintly
/// through parts in front.
pub(super) fn handle_adornment(dom: &Dom, id: InstanceId, size: Vec3) -> Vec<(PartInstance, Pose)> {
    let float = |name, default| dom.float_property(id, name).unwrap_or(default);
    let (radius, height) = (float("Radius", 1.0), float("Height", 1.0));
    let (shape, extent, centre) = match dom.class_name(id) {
        "BoxHandleAdornment" => (
            DrawShape::Part(PartShape::Block),
            dom.vector3_property(id, "Size").unwrap_or(Vec3::ONE),
            Vec3::ZERO,
        ),
        "SphereHandleAdornment" => (
            DrawShape::Part(PartShape::Ball),
            Vec3::splat(2.0 * radius),
            Vec3::ZERO,
        ),
        "CylinderHandleAdornment" => (
            DrawShape::Adorn(AdornShape::Tube {
                hole: AdornShape::hole(float("InnerRadius", 0.0), radius),
            }),
            Vec3::new(2.0 * radius, 2.0 * radius, height),
            Vec3::ZERO,
        ),
        "ConeHandleAdornment" => (
            DrawShape::Adorn(AdornShape::Cone),
            Vec3::new(2.0 * radius, 2.0 * radius, height),
            Vec3::new(0.0, 0.0, -height * 0.5),
        ),
        _ => return Vec::new(),
    };
    let piece = PartInstance {
        size: extent,
        ..piece(dom, id, shape)
    };
    if piece.transparency >= 1.0 {
        return Vec::new();
    }
    let cf = dom.cframe_property(id, "CFrame").unwrap_or_default();
    let shift = dom
        .vector3_property(id, "SizeRelativeOffset")
        .unwrap_or(Vec3::ZERO)
        * size
        * 0.5;
    let pose = Pose {
        offset: shift + cf.position + cf.rotation * centre,
        turn: cf.rotation,
    };
    // Inference: `AlwaysOnTop` follows the API reference (the MCP capture
    // leaves out what Studio draws on top); like Shading AlwaysOnTop, it
    // draws in exactly its colour, past the tone curve (measured).
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
        // red over grey 126, 152, 165 drew 190, 64, 75); over the visible
        // part the same colour changes nothing.
        let ghost = PartInstance {
            transparency: 1.0 - (1.0 - drawn.transparency) * 0.5,
            on_top: true,
            ..drawn
        };
        out.push((ghost, pose));
    }
    out
}
