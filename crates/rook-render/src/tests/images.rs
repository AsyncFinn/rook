//! Images replaced under their hash by larger ones (streamed in), drawn
//! where they were placed once the new ones are up.

use super::*;
use glam::Vec2;
use rook_content::decode::image::{Encoding, ImageFormat, mips};

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

/// A `size` × `size` image of one color with its mips, in BC1 blocks
/// where `blocks`.
fn solid(rgba: [u8; 4], size: u32, blocks: bool) -> Arc<Image> {
    let image = Image {
        width: size,
        height: size,
        format: ImageFormat::Png,
        encoding: Encoding::Rgba8,
        mips: mips(rgba.repeat((size * size) as usize), size, size),
        translucent: false,
        gui_size: (size, size),
    };
    Arc::new(if blocks {
        image.compressed(Encoding::Bc1)
    } else {
        image
    })
}

/// A white block whose color map is `hash`'s image.
fn mapped_block(hash: ImageHash) -> Scene {
    let mut block = part(PartShape::Block, 0.0, [1.0; 3], 0.0);
    block.maps = MaterialMaps {
        color: Some(hash),
        ..MaterialMaps::NONE
    };
    Scene {
        parts: vec![block],
        ..Scene::default()
    }
}

/// The middle pixel of a 32 × 32 frame of what's placed now.
fn drawn_middle(r: &mut Renderer) -> [u8; 3] {
    let camera = Camera::look_at(Vec3::new(0.0, 3.0, 6.0), Vec3::ZERO);
    let pixels = r.render_to_image(32, 32, &camera).unwrap();
    let i = (16 * 32 + 16) * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

fn reddish(c: [u8; 3]) -> bool {
    c[0] > 60 && c[0] > c[1].saturating_add(40) && c[0] > c[2].saturating_add(40)
}

fn greenish(c: [u8; 3]) -> bool {
    c[1] > 60 && c[1] > c[0].saturating_add(40) && c[1] > c[2].saturating_add(40)
}

/// The renderers to try: one with the device's features (an image table
/// where it has binding arrays) and one with none (a bind group per image
/// set, BC images decoded).
fn renderers() -> impl Iterator<Item = Renderer> {
    [
        Renderer::headless(),
        Renderer::headless_with(|_| wgpu::DeviceDescriptor::default()),
    ]
    .into_iter()
    .flatten()
}

/// A part drawing an image draws its larger replacement, another color
/// with more levels (BC1 where the device samples blocks), once it's up,
/// without being placed again; the old one draws until then.
#[test]
fn a_replaced_image_draws_where_it_was_placed() {
    for mut r in renderers() {
        let hash = ImageHash([11; 32]);
        r.take_images([(hash, solid(RED, 4, false))]);
        r.set_scene(&mapped_block(hash));
        r.finish_uploads();
        let red = drawn_middle(&mut r);
        assert!(reddish(red), "{red:?}");
        let placed = r.placements();
        let small = r.image_bytes();
        let blocks = r.block_compression;
        r.replace_images([(hash, solid(GREEN, 16, blocks))]);
        assert_eq!(r.image_bytes(), small, "the old image is kept until then");
        let green = drawn_middle(&mut r);
        assert!(greenish(green), "{green:?}");
        assert_eq!(r.placements(), placed);
        assert!(r.image_bytes() > small);
        assert!(!r.is_uploading());
    }
}

/// An image replaced before anything drew it is drawn as its replacement
/// the first time.
#[test]
fn a_parked_image_replaced_draws_the_new_one() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let hash = ImageHash([12; 32]);
    r.take_images([(hash, solid(RED, 4, false))]);
    let blocks = r.block_compression;
    r.replace_images([(hash, solid(GREEN, 16, blocks))]);
    assert_eq!(r.image_bytes(), 0, "parked, not uploaded");
    r.set_scene(&mapped_block(hash));
    r.finish_uploads();
    let green = drawn_middle(&mut r);
    assert!(greenish(green), "{green:?}");
}

/// An image replacing one never handed over is taken as `take_images`
/// takes it: parked until something draws it, then drawn the same.
#[test]
fn replacing_an_unknown_image_takes_it() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let (taken, replaced) = (ImageHash([13; 32]), ImageHash([14; 32]));
    r.take_images([(taken, solid(RED, 8, false))]);
    r.set_scene(&mapped_block(taken));
    r.finish_uploads();
    let expected = drawn_middle(&mut r);
    let bytes = r.image_bytes();
    r.replace_images([(replaced, solid(RED, 8, false))]);
    assert_eq!(r.image_bytes(), bytes, "parked, not uploaded");
    r.set_scene(&mapped_block(replaced));
    r.finish_uploads();
    assert_eq!(drawn_middle(&mut r), expected);
    assert_eq!(r.image_bytes(), 2 * bytes);
}

/// A screen GUI image draws its replacement once it's up.
#[test]
fn a_gui_image_draws_its_replacement() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let hash = [15; 32];
    r.set_scene(&Scene::default());
    let quad = GuiQuad {
        center: Vec2::splat(16.0),
        size: Vec2::splat(32.0),
        rotation: 0.0,
        color: [1.0; 4],
        clip: GuiQuad::NO_CLIP,
        paint: rook_types::Paint::Image(rook_types::ImagePaint {
            hash,
            uv: [0.0, 0.0, 1.0, 1.0],
            pixelated: false,
            tiled: false,
        }),
        shape: None,
        gradient: None,
    };
    r.take_images([(ImageHash(hash), solid(RED, 4, false))]);
    r.set_gui(Some(&Arc::from([Arc::<[GuiQuad]>::from([quad])])), None);
    r.finish_uploads();
    let red = drawn_middle(&mut r);
    assert!(red[0] > 240 && red[1] < 15 && red[2] < 15, "{red:?}");
    let blocks = r.block_compression;
    r.replace_images([(ImageHash(hash), solid(GREEN, 16, blocks))]);
    let green = drawn_middle(&mut r);
    assert!(
        green[1] > 240 && green[0] < 15 && green[2] < 15,
        "{green:?}"
    );
}
