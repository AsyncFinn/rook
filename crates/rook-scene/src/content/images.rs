//! The images the scene loaded. Their pixels go to the renderer at the
//! next hand-over ([`LoadedImages::take_fresh`]), which drops them once they're
//! uploaded; the scene keeps only what it reads itself: an image's size
//! (GUI layout, the cursor) and its average color (the sky's ambient). A
//! place's images once stayed in memory as a second copy for good, which
//! was gigabytes in a place with thousands of textures.

use std::sync::Arc;

use rook_content::decode::image::Image;
use rook_render::ImageHash;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ImageInfo {
    pub width: u32,
    pub height: u32,
    /// The 1 × 1 level's color, 0 to 1.
    pub average: [f32; 3],
}

impl ImageInfo {
    fn of(image: &Image) -> Self {
        let px = image.mips.last().expect("mip chain");
        Self {
            width: image.width,
            height: image.height,
            average: [px[0], px[1], px[2]].map(|c| f32::from(c) / 255.0),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct LoadedImages {
    known: FxHashMap<ImageHash, ImageInfo>,
    /// Loaded since the last hand-over.
    fresh: Vec<(ImageHash, Arc<Image>)>,
}

impl LoadedImages {
    /// Keep `image`'s size and color, and its pixels for the renderer,
    /// unless an image of the same bytes came before.
    pub(crate) fn add(&mut self, hash: ImageHash, image: Arc<Image>) {
        if self.known.contains_key(&hash) {
            return;
        }
        self.known.insert(hash, ImageInfo::of(&image));
        self.fresh.push((hash, image));
    }

    pub(crate) fn info(&self, hash: ImageHash) -> Option<ImageInfo> {
        self.known.get(&hash).copied()
    }

    pub(crate) fn len(&self) -> usize {
        self.known.len()
    }

    /// The pixels loaded since the last call, for the renderer to take.
    pub(crate) fn take_fresh(&mut self) -> Vec<(ImageHash, Arc<Image>)> {
        std::mem::take(&mut self.fresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scene hands an image's pixels on once and keeps none of them.
    #[test]
    fn pixels_are_handed_on_once_and_not_kept() {
        let mut images = LoadedImages::default();
        let image = Arc::new(Image {
            width: 2,
            height: 1,
            format: rook_content::decode::image::ImageFormat::Png,
            encoding: rook_content::decode::image::Encoding::Rgba8,
            mips: vec![vec![255, 0, 0, 255, 255, 0, 0, 255], vec![255, 0, 0, 255]],
            translucent: false,
        });
        let hash = ImageHash([1; 32]);
        images.add(hash, image.clone());
        images.add(hash, image.clone());
        assert_eq!(images.take_fresh().len(), 1);
        assert!(images.take_fresh().is_empty());
        assert_eq!(Arc::strong_count(&image), 1);
        let info = images.info(hash).expect("kept");
        assert_eq!(
            (info.width, info.height, info.average),
            (2, 1, [1.0, 0.0, 0.0])
        );
    }
}
