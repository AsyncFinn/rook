use std::sync::Arc;

use glam::Mat3;

use super::*;

fn part(shape: PartShape, x: f32, color: [f32; 3], transparency: f32) -> PartInstance {
    PartInstance {
        color,
        transparency,
        ..PartInstance::plain(
            DrawShape::Part(shape),
            Vec3::new(x, 0.0, 0.0),
            Mat3::IDENTITY,
            Vec3::splat(2.0),
        )
    }
}

/// A 2 × 2 image: red, green / blue, white.
fn image() -> Arc<Image> {
    let pixels = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255; 4],
    ];
    Arc::new(Image {
        width: 2,
        height: 2,
        format: rook_content::image::ImageFormat::Png,
        mips: rook_content::image::mips(pixels.concat(), 2, 2),
        translucent: false,
    })
}

/// Updating parts one by one draws what setting the whole scene draws:
/// moves, recolours, a shape change, opaque to transparent and back,
/// a part hidden, a new part in the slot it left, and decals and mesh
/// textures coming and going.
#[test]
fn updating_parts_draws_what_a_new_scene_draws() {
    // Rendering needs a GPU adapter; machines without one skip this.
    let Ok(mut incremental) = Renderer::headless() else {
        return;
    };
    let Ok(mut fresh) = Renderer::headless() else {
        return;
    };
    let red = [1.0, 0.0, 0.0];
    let green = [0.0, 1.0, 0.0];
    let blue = [0.0, 0.0, 1.0];
    let hash = ImageHash([7; 32]);
    let decal = |x: f32| PartInstance {
        layer: Layer::Image {
            image: hash,
            placement: ImagePlacement::Face(NormalId::Back),
            z: 1,
            on_transparent: x > 0.0,
        },
        ..part(PartShape::Block, x, [1.0; 3], 0.0)
    };
    let mut scene = Scene {
        parts: vec![
            part(PartShape::Block, -6.0, red, 0.0),
            part(PartShape::Ball, -3.0, green, 0.0),
            part(PartShape::Block, 0.0, blue, 0.5),
            part(PartShape::Cylinder, 3.0, red, 0.0),
            part(PartShape::Block, 6.0, green, 0.0),
            decal(-6.0),
        ],
        images: HashMap::from([(hash, image())]),
        ..Scene::default()
    };
    scene.parts[3].material = MaterialLook::Plastic;
    scene.parts[3].surfaces = [Some(SurfacePattern::Studs); 6];
    incremental.set_scene(&scene);
    let camera = Camera::look_at(Vec3::new(0.0, 3.0, 16.0), Vec3::ZERO);
    incremental.render_to_image(64, 32, &camera).unwrap();

    scene.parts[0].position.y = 1.0;
    scene.parts[0].color = blue;
    scene.parts[1].shape = DrawShape::Part(PartShape::Block);
    scene.parts[1].texture = Some(hash);
    scene.parts[2].transparency = 0.0;
    scene.parts[3].transparency = 0.25;
    scene.parts[4].transparency = 1.0;
    scene.parts[5] = decal(3.0);
    scene.parts.push(part(PartShape::Wedge, 9.0, red, 0.0));
    incremental.update_parts(&scene, &[0, 1, 2, 3, 4, 5, 6]);
    // The hidden part's slot is taken by a new one.
    scene.parts[4] = part(PartShape::Ball, -9.0, blue, 0.0);
    incremental.update_parts(&scene, &[4]);
    incremental.finish_uploads();

    fresh.set_scene(&scene);
    fresh.finish_uploads();
    assert_eq!(
        incremental.render_to_image(64, 32, &camera).unwrap(),
        fresh.render_to_image(64, 32, &camera).unwrap()
    );
}

/// A decal shows its image on its face, and a part waiting for its
/// image draws nothing until the upload.
#[test]
fn decals_draw_on_their_face() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let hash = ImageHash([1; 32]);
    let mut base = part(PartShape::Block, 0.0, [0.5; 3], 0.0);
    base.size = Vec3::new(4.0, 4.0, 1.0);
    let decal = PartInstance {
        layer: Layer::Image {
            image: hash,
            placement: ImagePlacement::Face(NormalId::Back),
            z: 1,
            on_transparent: false,
        },
        color: [1.0; 3],
        ..base
    };
    let scene = Scene {
        parts: vec![base, decal],
        images: HashMap::from([(hash, image())]),
        lighting: Lighting {
            ambient: [1.0; 3],
            outdoor_ambient: [0.0; 3],
            brightness: 0.0,
            ..Lighting::default()
        },
        ..Scene::default()
    };
    r.set_scene(&scene);
    assert!(r.is_uploading());
    // From +Z the Back face's image is upright: red top left, white
    // bottom right.
    let camera = Camera {
        fov_y: 20.0,
        ..Camera::look_at(Vec3::new(0.0, 0.0, 20.0), Vec3::ZERO)
    };
    let (w, h) = (64, 64);
    let pixels = r.render_to_image(w, h, &camera).unwrap();
    assert!(!r.is_uploading());
    let at = |pixels: &[u8], x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let pixels_after = r.render_to_image(w, h, &camera).unwrap();
    // The first frame uploaded the image, then drew it.
    assert_eq!(pixels, pixels_after);
    let top_left = at(&pixels_after, 24, 24);
    let bottom_right = at(&pixels_after, 40, 40);
    // Mostly red, mostly white (the 2 x 2 image is filtered).
    assert!(top_left[0] > top_left[1] + 100, "{top_left:?}");
    assert!(top_left[0] > top_left[2] + 100, "{top_left:?}");
    assert!(bottom_right.iter().all(|&c| c > 150), "{bottom_right:?}");
}
