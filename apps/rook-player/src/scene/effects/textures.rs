//! The images effects draw: the install's particle textures where it has
//! them (`textures/particles/`), else Rook's own.

use rook_content::decode::image::{Image, ImageFormat};

use super::*;

/// The image each effect draws, in [`Scene::images`].
#[derive(Debug, Clone, Copy)]
pub(in crate::scene) struct EffectTextures {
    pub(in crate::scene) fire: ImageHash,
    pub(in crate::scene) smoke: ImageHash,
    pub(in crate::scene) sparkles: ImageHash,
}

/// Rook's own textures' size.
const OWN: u32 = 64;

impl EffectTextures {
    /// Load the images from the install through `content` (or make
    /// Rook's own) and add them to `images`.
    pub(in crate::scene) fn load(
        content: &Resolver,
        images: &mut HashMap<ImageHash, Arc<Image>>,
    ) -> Self {
        let mut add = |file: &str, convert: fn(&mut [u8]), own: fn(f32, f32) -> f32| {
            let path = format!("textures/particles/{file}");
            let installed = content
                .resolve_local(&ContentRef::File(path.as_str().into()))
                .and_then(|f| {
                    let mut image = Image::decode(&f.bytes)
                        .map_err(
                            |e| tracing::warn!(target: "content", "{}: {e}", f.source.location),
                        )
                        .ok()?;
                    for level in &mut image.mips {
                        convert(level);
                    }
                    Some((blake3::hash(&f.bytes), image))
                });
            let (hash, image) = installed.unwrap_or_else(|| {
                let image = own_image(own);
                (blake3::hash(path.as_bytes()), image)
            });
            let hash = ImageHash(*hash.as_bytes());
            images.insert(hash, Arc::new(image));
            hash
        };
        Self {
            fire: add("fire_main.dds", unpremultiply, flame),
            smoke: add("smoke_main.dds", |_| {}, puff),
            sparkles: add("sparkles_main.dds", whiten, star),
        }
    }
}

/// The install's flames are premultiplied (their colour already scaled by
/// their alpha); Rook's particles take straight alpha.
fn unpremultiply(pixels: &mut [u8]) {
    for p in pixels.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]).max(1);
        for c in &mut p[..3] {
            *c = (u32::from(*c) * 255 / a).min(255) as u8;
        }
    }
}

/// The install's sparkle is its alpha, an eight-rayed star (its colour is
/// a round glow): Studio draws the star in SparkleColor (observed: sharp
/// rays).
fn whiten(pixels: &mut [u8]) {
    for p in pixels.as_chunks_mut::<4>().0 {
        p[..3].fill(255);
    }
}

/// A white image whose alpha is `alpha` at each point (-1 to 1 across).
fn own_image(alpha: fn(f32, f32) -> f32) -> Image {
    let mut pixels = image::RgbaImage::new(OWN, OWN);
    for (x, y, p) in pixels.enumerate_pixels_mut() {
        let u = (x as f32 + 0.5) / OWN as f32 * 2.0 - 1.0;
        let v = (y as f32 + 0.5) / OWN as f32 * 2.0 - 1.0;
        let a = (alpha(u, v).clamp(0.0, 1.0) * 255.0).round() as u8;
        *p = image::Rgba([255, 255, 255, a]);
    }
    Image::from_rgba(pixels, ImageFormat::Png, OWN).expect("non-empty image")
}

/// A soft round puff.
fn puff(u: f32, v: f32) -> f32 {
    let r = (u * u + v * v).sqrt();
    (1.0 - r).max(0.0).powi(2) * 0.5
}

/// A puff broken up into wisps, for flames.
fn flame(u: f32, v: f32) -> f32 {
    let wisps = 0.6 + 0.4 * ((u * 9.0).sin() * (v * 7.0 + u * 3.0).cos());
    puff(u, v) * wisps * 0.6
}

/// A bright centre with eight rays.
fn star(u: f32, v: f32) -> f32 {
    let r = (u * u + v * v).sqrt();
    let angle = v.atan2(u);
    let rays = (angle * 4.0).cos().abs().powi(24) * (1.0 - r).max(0.0);
    ((1.0 - r * 3.0).max(0.0) + rays * 0.8).min(1.0)
}
