//! What a union draws: its own geometry, from the asset its `AssetId`
//! names, loaded and drawn through the same path as mesh files.

use super::*;

/// The union classes.
pub(super) fn is_union(class: &str) -> bool {
    matches!(
        class,
        "UnionOperation" | "NegateOperation" | "IntersectOperation" | "PartOperation"
    )
}

/// What colour a NegateOperation draws in, whatever its Color and
/// `UsePartColor` (observed in Studio 0.742, 2026-10-14: a gold one and a
/// blue one both drew orange-red; this value is fitted against plain
/// parts under the same light, so an Approximation).
const NEGATE_COLOR: [f32; 3] = [252.0 / 255.0, 140.0 / 255.0, 90.0 / 255.0];

impl SceneCache {
    /// A union without a mesh child: its geometry, at its vertices times
    /// Size / `InitialSize` about the union's position, like a MeshPart
    /// (`mesh_part_scale`). With `UsePartColor` off its vertices keep the
    /// colours of the parts it was made from; on, it's the union's Color.
    /// Its Decals and Textures go on the box faces its vertices were cut
    /// from (observed). Returns how they go on it.
    pub(super) fn union(
        &mut self,
        dom: &Dom,
        id: InstanceId,
        size: Vec3,
        out: &mut PartInstance,
    ) -> Decals {
        let url = content_url(dom, id, &["Content", "AssetId"]);
        // Without an asset there's no geometry: Studio draws nothing (a
        // union saved without `AssetId` or `MeshData`, observed).
        if url.is_empty() {
            out.transparency = 1.0;
            return Decals::None;
        }
        // Like a new mesh file: nothing while it loads (Inference) and
        // nothing if it fails (observed: an asset that isn't a union).
        let FileDraw::Mesh(hash) = self.file(id, id, &url) else {
            out.transparency = 1.0;
            return Decals::None;
        };
        let initial = dom
            .vector3_property(id, "InitialSize")
            .unwrap_or(Vec3::ZERO);
        out.shape = DrawShape::File(hash);
        out.size = mesh_part_scale(size, initial);
        if dom.class_name(id) == "NegateOperation" {
            out.color = NEGATE_COLOR;
        } else {
            out.vertex_colors = !dom.bool_property(id, "UsePartColor").unwrap_or(false);
        }
        Decals::Faces
    }
}
