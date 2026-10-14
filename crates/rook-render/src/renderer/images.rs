//! The images each draw binds (group 2): its image (a mesh texture, a
//! decal or a skybox face) and its material's colour, normal, roughness
//! and metalness maps. Each set gets one bind group, made once all its
//! images are on the GPU and kept with them.

use super::*;

/// A draw's images, in binding order: image, colour, normal, roughness,
/// metalness.
pub(crate) type ImageSet = [Option<ImageHash>; 5];

/// What stands in for a missing image: white for the image and colour
/// (sRGB) and for roughness and metalness (linear, so the part's own
/// values count in full), and a flat normal.
#[derive(Debug)]
pub(crate) struct Defaults {
    pub(crate) white: GpuImage,
    pub(crate) flat_normal: GpuImage,
}

impl Defaults {
    pub(crate) fn new(cx: &UploadContext<'_>) -> Self {
        Self {
            white: GpuImage::solid(cx, "rook white", [255; 4]),
            flat_normal: GpuImage::solid(cx, "rook flat normal", [128, 128, 255, 255]),
        }
    }
}

/// The image set an entry draws with.
pub(crate) fn image_set(part: &PartInstance) -> ImageSet {
    let image = match part.layer {
        Layer::Base => part.texture,
        Layer::Image { image, .. } => Some(image),
    };
    let m = part.maps;
    [image, m.color, m.normal, m.roughness, m.metalness]
}

impl Renderer {
    /// Make the bind group of `set`, whose images must all be on the GPU.
    pub(crate) fn ensure_group(&mut self, set: ImageSet) {
        if self.groups.contains_key(&set) {
            return;
        }
        let d = &self.defaults;
        let fallback = [&d.white, &d.white, &d.flat_normal, &d.white, &d.white];
        let mut images = fallback;
        for (slot, hash) in set.iter().enumerate() {
            if let Some(gpu) = hash.and_then(|h| self.images.get(&h)) {
                images[slot] = gpu;
            }
        }
        let group = images_bind_group(&self.upload_context(), images);
        self.groups.insert(set, group);
    }

    /// The bind group of a draw's images (the defaults' when it hasn't
    /// been made).
    pub(crate) fn group(&self, set: &ImageSet) -> &wgpu::BindGroup {
        self.groups
            .get(set)
            .unwrap_or_else(|| &self.groups[&[None; 5]])
    }
}
