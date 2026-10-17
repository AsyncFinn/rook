use std::collections::HashMap;
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
        format: rook_content::decode::image::ImageFormat::Png,
        mips: rook_content::decode::image::mips(pixels.concat(), 2, 2),
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
    scene.parts[3].material = Material::Plastic;
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

/// Water's phase sums `WaterWaveSpeed` over time: it stops where it is at
/// speed 0 and goes on from there (as Studio's water does), and a clock
/// going back moves nothing.
#[test]
fn water_phase_follows_wave_speed() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let mut scene = Scene::default();
    r.set_scene(&scene);
    r.set_time(1.0);
    assert!((r.water_phase - 10.0).abs() < 1e-4, "{}", r.water_phase);
    scene.terrain.wave_speed = 0.0;
    r.update_parts(&scene, &[]);
    r.set_time(3.0);
    assert!((r.water_phase - 10.0).abs() < 1e-4);
    scene.terrain.wave_speed = 4.0;
    r.update_parts(&scene, &[]);
    r.set_time(3.5);
    assert!((r.water_phase - 12.0).abs() < 1e-4);
    r.set_time(0.0);
    assert!((r.water_phase - 12.0).abs() < 1e-4);
}

/// A part behind the camera, culled from the main pass, still shadows the
/// ground in view: the shadow pass draws what the sun sees.
#[test]
fn parts_out_of_view_still_cast_shadows() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let mut ground = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    ground.position.y = -0.5;
    ground.size = Vec3::new(200.0, 1.0, 200.0);
    // A tower 15 studs behind the camera, with the sun behind it (a
    // cylinder, so it's a batch of its own).
    let mut tower = part(PartShape::Cylinder, 0.0, [0.8; 3], 0.0);
    tower.position = Vec3::new(0.0, 20.0, 15.0);
    // Standing up: a cylinder's axis is its X.
    tower.rotation = Mat3::from_rotation_z(std::f32::consts::FRAC_PI_2);
    tower.size = Vec3::new(40.0, 4.0, 4.0);
    let mut scene = Scene {
        parts: vec![ground],
        lighting: Lighting {
            sun_direction: Vec3::new(0.0, 1.0, 1.0).normalize(),
            ..Lighting::default()
        },
        ..Scene::default()
    };
    let camera = Camera::look_at(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, 0.0, -20.0));
    r.set_scene(&scene);
    let lit = r.render_to_image(64, 64, &camera).unwrap();
    scene.parts.push(tower);
    r.set_scene(&scene);
    let shadowed = r.render_to_image(64, 64, &camera).unwrap();
    assert_eq!(r.draw_counts().main.draws, 1, "the tower is out of view");
    assert_eq!(r.draw_counts().shadow.draws, 2);
    assert_ne!(lit, shadowed);
}

/// A small mesh file: a square of two triangles facing +Z, `side` wide.
fn square(side: f32) -> Arc<Mesh> {
    let h = side / 2.0;
    Arc::new(Mesh {
        positions: vec![[-h, -h, 0.0], [h, -h, 0.0], [h, h, 0.0], [-h, h, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        indices: vec![0, 1, 2, 0, 2, 3],
        ..Mesh::default()
    })
}

/// A scene drawing with everything that binds images or meshes: mesh
/// files, a mesh texture, material maps, a decal, transparent parts,
/// Glass, a ForceField and a skybox.
fn busy_scene() -> Scene {
    let (a, b) = (ImageHash([1; 32]), ImageHash([2; 32]));
    let (m1, m2) = (MeshHash([3; 32]), MeshHash([4; 32]));
    let mut parts = vec![
        part(PartShape::Block, -6.0, [1.0, 0.0, 0.0], 0.0),
        part(PartShape::Ball, -3.0, [0.0, 1.0, 0.0], 0.0),
        part(PartShape::Block, 0.0, [0.0, 0.0, 1.0], 0.5),
        part(PartShape::Cylinder, 3.0, [1.0, 1.0, 0.0], 0.0),
        part(PartShape::Wedge, 6.0, [0.0, 1.0, 1.0], 0.0),
    ];
    parts[1].texture = Some(a);
    parts[3].maps = MaterialMaps {
        color: Some(b),
        normal: Some(a),
        ..MaterialMaps::NONE
    };
    let mut glass = part(PartShape::Block, -1.5, [0.5, 0.8, 1.0], 0.3);
    glass.material = Material::Glass;
    let mut field = part(PartShape::Ball, 1.5, [1.0, 0.2, 0.2], 0.0);
    field.material = Material::ForceField;
    parts.extend([glass, field]);
    for (i, mesh) in [m1, m2, m1].into_iter().enumerate() {
        let mut p = part(PartShape::Block, -4.0 + i as f32 * 4.0, [0.9; 3], 0.0);
        p.shape = DrawShape::File(mesh);
        p.position.y = 3.0;
        p.size = Vec3::ONE;
        p.texture = (i == 1).then_some(b);
        parts.push(p);
    }
    parts.push(PartInstance {
        layer: Layer::Image {
            image: b,
            placement: ImagePlacement::Face(NormalId::Back),
            z: 1,
            on_transparent: false,
        },
        ..part(PartShape::Block, -6.0, [1.0; 3], 0.0)
    });
    Scene {
        parts,
        images: HashMap::from([(a, image()), (b, image())]),
        meshes: HashMap::from([(m1, square(2.0)), (m2, square(1.5))]),
        lighting: Lighting {
            skybox: Some([a, b, a, b, a, b]),
            ..Lighting::default()
        },
        ..Scene::default()
    }
}

/// Every image in one table and each pipeline's draws in a few
/// multi-draws draw exactly what a bind group per image set and a draw
/// call per batch draw (the way for adapters without binding arrays or
/// first instances in indirect draws).
#[test]
fn table_and_multi_draws_draw_what_bound_sets_draw() {
    let Ok(mut fast) = Renderer::headless() else {
        return;
    };
    let Ok(mut plain) = Renderer::headless_with(|_| wgpu::DeviceDescriptor::default()) else {
        return;
    };
    assert!(matches!(
        plain.image_binding,
        renderer::ImageBinding::Sets(_)
    ));
    assert!(!plain.multi_draw);
    let scene = busy_scene();
    let camera = Camera::look_at(Vec3::new(0.0, 4.0, 14.0), Vec3::ZERO);
    let mut frames = Vec::new();
    for r in [&mut fast, &mut plain] {
        r.set_scene(&scene);
        r.finish_uploads();
        frames.push(r.render_to_image(96, 64, &camera).unwrap());
    }
    assert_eq!(frames[0], frames[1]);
    let (fast, plain) = (fast.draw_counts().main, plain.draw_counts().main);
    assert_eq!(plain.calls, plain.draws);
    assert_eq!(fast.draws, plain.draws);
    if fast.calls < fast.draws {
        // Opaque, layer and the transparent passes (three for Glass) each
        // need a call.
        assert!(fast.calls <= 7, "{fast:?}");
    }
}

/// A mesh dropped from the pool leaves room that a later mesh takes, and
/// both draw as they would in a fresh renderer.
#[test]
fn dropped_meshes_make_room_for_others() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let mut scene = busy_scene();
    let camera = Camera::look_at(Vec3::new(0.0, 4.0, 14.0), Vec3::ZERO);
    r.set_scene(&scene);
    r.finish_uploads();
    r.render_to_image(96, 64, &camera).unwrap();
    // The first square goes, and a new square comes in its place.
    let (old, new) = (MeshHash([3; 32]), MeshHash([5; 32]));
    r.drop_mesh(old);
    scene.meshes.remove(&old);
    scene.meshes.insert(new, square(2.0));
    let moved: Vec<usize> = (0..scene.parts.len())
        .filter(|&i| scene.parts[i].shape == DrawShape::File(old))
        .collect();
    for &i in &moved {
        scene.parts[i].shape = DrawShape::File(new);
    }
    r.update_parts(&scene, &moved);
    r.finish_uploads();
    let Ok(mut fresh) = Renderer::headless() else {
        return;
    };
    fresh.set_scene(&scene);
    fresh.finish_uploads();
    assert_eq!(
        r.render_to_image(96, 64, &camera).unwrap(),
        fresh.render_to_image(96, 64, &camera).unwrap()
    );
}
