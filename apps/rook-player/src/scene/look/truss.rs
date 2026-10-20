//! A TrussPart's bars.

use super::*;

/// A TrussPart without a mesh: its bars, along its long axis from its low
/// end (`rook_types::TrussLook`), in the part's colour, material and
/// transparency (observed in Studio 0.742, 2026-10-13: Color, Neon,
/// Reflectance and Transparency show as on any part, and Wood its wood).
/// Its Decals and Textures and its surfaces don't show (observed).
pub(super) fn truss(
    dom: &Dom,
    id: InstanceId,
    cf: &CFrame,
    size: Vec3,
    mut out: PartInstance,
    pose: &mut Pose,
) -> PartInstance {
    // Files save `Style` as `style`, which is what the DOM holds (scripts
    // see it as `Style`).
    let style = TrussStyle::from_u32(dom.enum_property(id, "style").unwrap_or(0));
    let look = TrussLook::of(size, style);
    out.shape = DrawShape::Truss(look.shape);
    out.size = Vec3::ONE;
    out.position = cf.position + cf.rotation * look.offset;
    out.rotation = cf.rotation * look.turn;
    pose.offset = look.offset;
    pose.turn = look.turn;
    out
}
