use super::*;

/// A block-compressed color map draws the same where the device samples
/// blocks and where it gets them decoded (a device without BC).
#[test]
fn block_compressed_maps_draw_the_same_decoded() {
    use rook_content::decode::image::{Encoding, ImageFormat, mips};
    let Ok(mut fast) = Renderer::headless() else {
        return;
    };
    let Ok(mut plain) = Renderer::headless_with(|_| wgpu::DeviceDescriptor::default()) else {
        return;
    };
    assert!(!plain.block_compression);
    let flat = [200, 120, 40, 255].repeat(64);
    let map = Image {
        width: 8,
        height: 8,
        format: ImageFormat::Png,
        encoding: Encoding::Rgba8,
        mips: mips(flat, 8, 8),
        translucent: false,
    }
    .compressed(Encoding::Bc1);
    let hash = ImageHash([5; 32]);
    let mut block = part(PartShape::Block, 0.0, [1.0; 3], 0.0);
    block.maps = MaterialMaps {
        color: Some(hash),
        ..MaterialMaps::NONE
    };
    let scene = Scene {
        parts: vec![block],
        ..Scene::default()
    };
    let map = Arc::new(map);
    let camera = Camera::look_at(Vec3::new(0.0, 3.0, 6.0), Vec3::ZERO);
    let mut frames = Vec::new();
    for r in [&mut fast, &mut plain] {
        r.take_images([(hash, map.clone())]);
        r.set_scene(&scene);
        r.finish_uploads();
        frames.push(r.render_to_image(48, 32, &camera).unwrap());
    }
    assert!(
        frames[0]
            .iter()
            .zip(&frames[1])
            .all(|(x, y)| x.abs_diff(*y) <= 2)
    );
}

/// A flat normal map of one `tilt`, and the hash that keeps it apart from
/// other maps.
fn tilted_map(tilt: [f32; 2]) -> (ImageHash, Arc<Image>) {
    use rook_content::decode::image::{Encoding, ImageFormat, mips};
    let t = Vec3::new(tilt[0], tilt[1], 1.0).normalize();
    let texel = (t * 0.5 + 0.5) * 255.0;
    let texel = [texel.x as u8, texel.y as u8, texel.z as u8, 255];
    let mut hash = ImageHash([6; 32]);
    hash.0[..3].copy_from_slice(&texel[..3]);
    let map = Image {
        width: 8,
        height: 8,
        format: ImageFormat::Png,
        encoding: Encoding::Rgba8,
        mips: mips(texel.repeat(64), 8, 8),
        translucent: false,
    };
    (hash, Arc::new(map))
}

/// The sum of the middle pixel's channels: `block` drawn with `maps`
/// under a sun from `sun`, seen by `camera`.
fn lit_middle(
    r: &mut Renderer,
    block: PartInstance,
    maps: Vec<(ImageHash, Arc<Image>)>,
    sun: Vec3,
    camera: &Camera,
) -> u32 {
    r.take_images(maps);
    r.set_scene(&Scene {
        parts: vec![block],
        lighting: Lighting {
            sun_direction: sun.normalize(),
            ..Lighting::default()
        },
        ..Scene::default()
    });
    r.finish_uploads();
    let pixels = r.render_to_image(16, 16, camera).unwrap();
    let at = (8 * 16 + 8) * 4;
    pixels[at..at + 3].iter().map(|&c| u32::from(c)).sum()
}

/// A normal map tilted towards the sun lights its face more than one
/// tilted away, across the image (X right) and up it (Y up): the frame
/// follows the image whichever way the screen's derivatives run (WGSL's
/// `dpdy` runs down, which once sank every bump).
#[test]
fn normal_maps_tilt_towards_the_image_right_and_up() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    // The Back face (+Z) faces the camera: its image's right is +X, its up +Y.
    let camera = Camera::look_at(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO);
    let mut lit = |tilt: [f32; 2], sun: Vec3| {
        let map = tilted_map(tilt);
        let mut block = part(PartShape::Block, 0.0, [0.3; 3], 0.0);
        block.maps = MaterialMaps {
            normal: Some(map.0),
            ..MaterialMaps::NONE
        };
        lit_middle(&mut r, block, vec![map], sun, &camera)
    };
    let from_right = Vec3::new(0.8, 0.0, 0.6);
    assert!(lit([0.7, 0.0], from_right) > lit([-0.7, 0.0], from_right) + 20);
    let from_above = Vec3::new(0.0, 0.8, 0.6);
    assert!(lit([0.0, 0.7], from_above) > lit([0.0, -0.7], from_above) + 20);
}

/// Normal maps count in full up close and not at all past their fade.
#[test]
fn normal_maps_fade_out_with_depth() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let from_right = Vec3::new(0.8, 0.0, 0.6);
    let mut relief = |distance: f32| {
        let camera = Camera::look_at(Vec3::new(0.0, 0.0, 200.0 + distance), Vec3::ZERO);
        let mut lit = |tilt: [f32; 2]| {
            let map = tilted_map(tilt);
            let mut block = part(PartShape::Block, 0.0, [0.3; 3], 0.0);
            block.size = Vec3::splat(400.0);
            block.maps = MaterialMaps {
                normal: Some(map.0),
                ..MaterialMaps::NONE
            };
            lit_middle(&mut r, block, vec![map], from_right, &camera)
        };
        lit([0.7, 0.0]).abs_diff(lit([-0.7, 0.0]))
    };
    assert!(relief(6.0) > 20);
    assert!(relief(2000.0) <= 3);
}

/// A detail normal map adds its slopes to the normal map's, as far as its
/// strength says, and takes its strength from `NormalDetail`.
#[test]
fn detail_normal_maps_add_to_the_normal_map() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let camera = Camera::look_at(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO);
    let from_right = Vec3::new(0.8, 0.0, 0.6);
    let mut lit = |tilt: f32, strength: f32| {
        let flat = tilted_map([0.0, 0.0]);
        let detail = tilted_map([tilt, 0.0]);
        let mut block = part(PartShape::Block, 0.0, [0.3; 3], 0.0);
        block.maps = MaterialMaps {
            normal: Some(flat.0),
            detail_normal: Some(detail.0),
            detail: NormalDetail {
                repeats: 7.0,
                strength,
            },
            alpha: MapAlpha::TintMask,
            ..MaterialMaps::NONE
        };
        lit_middle(&mut r, block, vec![flat, detail], from_right, &camera)
    };
    assert!(lit(0.7, 1.0) > lit(-0.7, 1.0) + 20);
    // Stronger, the slopes differ more.
    let weak = lit(0.7, 0.25).abs_diff(lit(-0.7, 0.25));
    let strong = lit(0.7, 1.5).abs_diff(lit(-0.7, 1.5));
    assert!(strong > weak + 10, "{strong} against {weak}");
}
