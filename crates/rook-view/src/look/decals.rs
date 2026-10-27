//! A part's Decals and Textures, drawn over it as image layers.

use super::*;

impl SceneCache {
    /// A part's Decals and Textures, in child order, each drawn over `part`
    /// once its image has loaded. While an image loads, or after it fails,
    /// it draws nothing: a Decal whose `Texture` changes doesn't keep its
    /// old image (observed in Studio 0.742, 2026-10-13).
    pub(super) fn images(
        &mut self,
        dom: &Dom,
        id: InstanceId,
        part: &PartInstance,
        decals: Decals,
    ) -> Vec<PartInstance> {
        let mut out = Vec::new();
        for &child in dom.children(id) {
            let class = dom.class_name(child);
            if class != "Decal" && class != "Texture" {
                continue;
            }
            let url = content_url(dom, child, &["TextureContent", "Texture"]);
            if url.is_empty() {
                continue;
            }
            let FileState::Loaded(hash) = self.file_state(id, &url, Want::Image) else {
                continue;
            };
            let face = dom
                .enum_property(child, "Face")
                .and_then(NormalId::from_u32)
                .unwrap_or(NormalId::Front);
            let placement = match (decals, class) {
                (Decals::MeshUv, _) => ImagePlacement::MeshUv,
                (Decals::Faces, "Texture") => ImagePlacement::Tiled(
                    face,
                    Tiling {
                        studs_per_tile: Vec2::new(
                            dom.float_property(child, "StudsPerTileU").unwrap_or(2.0),
                            dom.float_property(child, "StudsPerTileV").unwrap_or(2.0),
                        ),
                        offset: Vec2::new(
                            dom.float_property(child, "OffsetStudsU").unwrap_or(0.0),
                            dom.float_property(child, "OffsetStudsV").unwrap_or(0.0),
                        ),
                    },
                ),
                _ => ImagePlacement::Face(face),
            };
            out.push(PartInstance {
                color: dom
                    .color3_property(child, "Color3")
                    .map(Color3::to_array)
                    .unwrap_or([1.0; 3]),
                transparency: super::transparency(dom, child),
                texture: None,
                surfaces: [None; 6],
                material: Material::SmoothPlastic,
                maps: MaterialMaps::NONE,
                layer: Layer::Image {
                    image: ImageHash(hash),
                    placement,
                    z: dom.float_property(child, "ZIndex").unwrap_or(1.0) as i32,
                    on_transparent: part.transparency > 0.0 && part.transparency < 1.0,
                },
                ..*part
            });
        }
        out
    }
}
