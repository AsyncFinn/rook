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
    /// The size GUI measures the image in (whatever size its pixels came
    /// in) and its color.
    fn of(image: &Image) -> Self {
        let [r, g, b, _] = image.average();
        Self {
            width: image.gui_size.0,
            height: image.gui_size.1,
            average: [r, g, b].map(|c| f32::from(c) / 255.0),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct LoadedImages {
    known: FxHashMap<ImageHash, ImageInfo>,
    /// Loaded since the last hand-over.
    fresh: Vec<(ImageHash, Arc<Image>)>,
    /// Larger versions of images the renderer already took, to replace
    /// them under the same hash.
    upgraded: Vec<(ImageHash, Arc<Image>)>,
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

    /// A larger version of a loaded image (a texture streaming in): it
    /// takes the place of the one the renderer hasn't taken yet, or goes
    /// to the renderer to replace it. Its size for GUI and its color stay
    /// the image's.
    pub(crate) fn upgrade(&mut self, hash: ImageHash, image: Arc<Image>) {
        match self.fresh.iter_mut().find(|(h, _)| *h == hash) {
            Some(slot) => slot.1 = image,
            None => self.upgraded.push((hash, image)),
        }
    }

    /// The larger versions since the last call, for the renderer.
    pub(crate) fn take_upgraded(&mut self) -> Vec<(ImageHash, Arc<Image>)> {
        std::mem::take(&mut self.upgraded)
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
            gui_size: (2, 1),
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

    fn flat(side: u32) -> Arc<Image> {
        let pixels = vec![[0, 0, 255, 255]; (side * side) as usize].concat();
        Arc::new(Image {
            width: side,
            height: side,
            format: rook_content::decode::image::ImageFormat::Ktx2,
            encoding: rook_content::decode::image::Encoding::Rgba8,
            mips: rook_content::decode::image::mips(pixels, side, side),
            translucent: false,
            gui_size: (64, 64),
        })
    }

    /// A texture streaming in: a larger version takes the place of one the
    /// renderer hasn't taken, and goes to the renderer to replace one it
    /// has, its size for GUI unchanged.
    #[test]
    fn larger_versions_replace_an_image() {
        let mut images = LoadedImages::default();
        let hash = ImageHash([2; 32]);
        images.add(hash, flat(2));
        images.upgrade(hash, flat(4));
        let fresh = images.take_fresh();
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].1.width, 4);
        assert!(images.take_upgraded().is_empty());
        images.upgrade(hash, flat(8));
        let upgraded = images.take_upgraded();
        assert_eq!((upgraded.len(), upgraded[0].1.width), (1, 8));
        assert!(images.take_fresh().is_empty());
        assert_eq!(images.info(hash).map(|i| i.width), Some(64));
    }
}
