use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat3, Vec2};
use rook_types::SurfacePattern;

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
        encoding: rook_content::decode::image::Encoding::Rgba8,
        mips: rook_content::decode::image::mips(pixels.concat(), 2, 2),
        translucent: false,
    })
}

/// Updating parts one by one draws what setting the whole scene draws:
/// moves, recolors, a shape change, opaque to transparent and back,
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
        // The shadow map: voxel shadows take moved parts only every half
        // second, so they'd still show where the parts were.
        lighting: Lighting {
            prioritize_quality: true,
            ..Lighting::default()
        },
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
    // Parts that only move: opaque, waiting for its image, transparent,
    // and a decal on a transparent part.
    for i in [0, 1, 3, 5] {
        scene.parts[i].position.x += 0.5;
        scene.parts[i].rotation = Mat3::from_rotation_y(0.3);
    }
    incremental.move_parts(&scene, &[0, 1, 3, 5]);
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
            sun_color: [0.0; 3],
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
            prioritize_quality: true,
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

/// Without PrioritizeLightingQuality the sun shadows through the light
/// voxels, with no shadow pass: a slab over the ground darkens it, and the
/// same slab above the voxels' reach casts nothing, as in Studio.
#[test]
fn without_quality_the_voxels_shadow_the_sun() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let mut ground = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    ground.position.y = -0.5;
    ground.size = Vec3::new(200.0, 1.0, 200.0);
    let mut slab = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    slab.size = Vec3::new(40.0, 4.0, 40.0);
    let mut scene = Scene {
        parts: vec![ground],
        lighting: Lighting {
            sun_direction: Vec3::Y,
            ..Lighting::default()
        },
        ..Scene::default()
    };
    let camera = Camera::look_at(Vec3::new(0.0, 30.0, 30.0), Vec3::ZERO);
    r.set_scene(&scene);
    let lit = r.render_to_image(64, 64, &camera).unwrap();
    scene.parts.push(slab);
    scene.parts[1].position.y = 20.0;
    r.set_scene(&scene);
    let shadowed = r.render_to_image(64, 64, &camera).unwrap();
    assert_eq!(r.draw_counts().shadow.draws, 0);
    assert_ne!(lit, shadowed);
    // Out of view, and above the voxels around the camera.
    scene.parts[1].position.y = 400.0;
    r.set_scene(&scene);
    assert_eq!(r.render_to_image(64, 64, &camera).unwrap(), lit);
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

/// A mesh file of two levels of detail drawn near, then past Studio's
/// first threshold: the far one draws its coarser level, and the decal on
/// it follows its part.
#[test]
fn far_meshes_draw_their_lower_level() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let (mesh, picture) = (MeshHash([5; 32]), ImageHash([6; 32]));
    let mut two = Arc::unwrap_or_clone(square(2.0));
    two.indices.extend([0, 1, 2]);
    two.levels = rook_content::decode::mesh::Levels {
        starts: vec![6],
        high_quality: 2,
    };
    let base = PartInstance {
        fidelity: Fidelity::Automatic,
        ..PartInstance::plain(DrawShape::File(mesh), Vec3::ZERO, Mat3::IDENTITY, Vec3::ONE)
    };
    let decal = PartInstance {
        layer: Layer::Image {
            image: picture,
            placement: ImagePlacement::MeshUv,
            z: 1,
            on_transparent: false,
        },
        fidelity: Fidelity::Precise,
        drawn_over: Some(0),
        ..base
    };
    r.set_scene(&Scene {
        parts: vec![base, decal],
        images: HashMap::from([(picture, image())]),
        meshes: HashMap::from([(mesh, Arc::new(two))]),
        ..Scene::default()
    });
    let mut triangles = |z: f32| {
        let camera = Camera::look_at(Vec3::new(0.0, 0.0, z), Vec3::ZERO);
        r.render_to_image(16, 16, &camera).unwrap();
        r.draw_counts().main.triangles
    };
    assert_eq!(triangles(100.0), 4);
    // 250 studs from the box, which reaches 1 stud out of 0.
    assert_eq!(triangles(252.0), 2);
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
        images: HashMap::from([(hash, Arc::new(map))]),
        ..Scene::default()
    };
    let camera = Camera::look_at(Vec3::new(0.0, 3.0, 6.0), Vec3::ZERO);
    let mut frames = Vec::new();
    for r in [&mut fast, &mut plain] {
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

/// A Highlight fills its part in exactly its color over the finished
/// frame and outlines it, through a wall when on top; occluded, the wall
/// hides both.
#[test]
fn highlights_fill_and_outline_through_walls_when_on_top() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let highlight = |on_top| Highlight {
        key: 1,
        fill: [0.0, 1.0, 0.0, 1.0],
        outline: [0.0, 0.0, 1.0, 1.0],
        on_top,
    };
    let mut wall = part(PartShape::Block, 0.0, [0.5; 3], 0.0);
    wall.position.z = 4.0;
    wall.size = Vec3::new(20.0, 20.0, 1.0);
    let mut scene = Scene {
        parts: vec![
            PartInstance {
                highlight: Some(highlight(true)),
                ..part(PartShape::Block, 0.0, [1.0, 0.0, 0.0], 0.0)
            },
            wall,
        ],
        ..Scene::default()
    };
    r.set_scene(&scene);
    let camera = Camera {
        fov_y: 20.0,
        ..Camera::look_at(Vec3::new(0.0, 0.0, 20.0), Vec3::ZERO)
    };
    let (w, h) = (64, 64);
    let at = |pixels: &[u8], x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let pixels = r.render_to_image(w, h, &camera).unwrap();
    assert_eq!(at(&pixels, 32, 32), [0, 255, 0]);
    let row: Vec<[u8; 3]> = (0..w).map(|x| at(&pixels, x, 32)).collect();
    let outline = row.iter().filter(|&&c| c == [0, 0, 255]).count();
    // Two pixels at either side.
    assert_eq!(outline, 4, "{row:?}");

    scene.parts[0].highlight = Some(highlight(false));
    r.update_parts(&scene, &[0]);
    let pixels = r.render_to_image(w, h, &camera).unwrap();
    assert_eq!(at(&pixels, 32, 32), at(&pixels, 4, 4));
    assert!(!(0..w).any(|x| at(&pixels, x, 32) == [0, 0, 255]));
}

/// A SurfaceGui's canvas shows on its face, sharp and unlit at its own
/// color through the curve; a BillboardGui behind the part is hidden
/// unless it's AlwaysOnTop.
#[test]
fn world_gui_draws_in_the_world() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let mut base = part(PartShape::Block, 0.0, [0.5; 3], 0.0);
    base.size = Vec3::new(4.0, 4.0, 1.0);
    r.set_scene(&Scene {
        parts: vec![base],
        lighting: Lighting {
            ambient: [1.0; 3],
            outdoor_ambient: [0.0; 3],
            sun_color: [0.0; 3],
            ..Lighting::default()
        },
        ..Scene::default()
    });
    let fill = |center: Vec2, size: Vec2, color: [f32; 3]| GuiQuad {
        center,
        size,
        rotation: 0.0,
        color: [color[0], color[1], color[2], 1.0],
        clip: GuiQuad::NO_CLIP,
        paint: rook_types::Paint::Solid,
        shape: None,
        gradient: None,
    };
    let canvas = |quads: Vec<GuiQuad>, size: f32, on_top: bool| WorldCanvas {
        quads,
        size: Vec2::splat(size),
        light_influence: 0.0,
        brightness: 1.0,
        always_on_top: on_top,
        max_distance: f32::INFINITY,
        untoned: false,
    };
    // Red on the left half of a canvas 4 pixels across, a stud each.
    let surface = canvas(
        vec![fill(
            Vec2::new(1.0, 2.0),
            Vec2::new(2.0, 4.0),
            [1.0, 0.0, 0.0],
        )],
        4.0,
        false,
    );
    let square = |color| vec![fill(Vec2::splat(5.0), Vec2::splat(10.0), color)];
    let hidden = canvas(square([0.0, 1.0, 0.0]), 10.0, false);
    let on_top = canvas(square([0.0, 0.0, 1.0]), 10.0, true);
    let canvases = [surface, hidden, on_top].map(Arc::new);
    r.set_gui(&Arc::from([]), &canvases, &Default::default());
    let billboard = |y: f32| WorldPlace::Billboard {
        anchor: Vec3::new(1.0, y, -5.0),
        studs: Vec3::ZERO,
        size_studs: Vec2::ZERO,
        size_pixels: Vec2::splat(6.0),
        shift: Vec2::ZERO,
    };
    // The Back face (+Z), its top left at −X +Y.
    r.place_world_gui(vec![
        Some(WorldPlace::Surface {
            origin: Vec3::new(-2.0, 2.0, 0.5),
            right: Vec3::X,
            down: Vec3::NEG_Y,
        }),
        Some(billboard(1.0)),
        Some(billboard(-1.0)),
    ]);
    let camera = Camera {
        fov_y: 20.0,
        ..Camera::look_at(Vec3::new(0.0, 0.0, 20.0), Vec3::ZERO)
    };
    let (w, h) = (64, 64);
    let pixels = r.render_to_image(w, h, &camera).unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    // The face spans pixels 13 to 51; the billboards sit 7 pixels from
    // the middle at depth 25.
    let red = at(22, 32);
    assert!(red[0] > 200 && red[1] < 30 && red[2] < 30, "{red:?}");
    let part = at(42, 32);
    assert!(part[0] < 200 && part[0].abs_diff(part[1]) < 8, "{part:?}");
    // Its edge is sharp, however few canvas pixels are stretched over it.
    assert_eq!(at(30, 32), red);
    assert_eq!(at(34, 32), part);
    let behind = at(39, 25);
    assert!(behind[1].abs_diff(behind[0]) < 8, "{behind:?}");
    let over = at(39, 39);
    assert!(over[2] > 200 && over[0] < 30, "{over:?}");
}

/// Screen GUI images draw their part of the image, mirrored, tiled and
/// tinted, with nearest texels when pixelated; one whose image isn't
/// there draws nothing.
#[test]
fn gui_images_draw_their_texels() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    r.set_scene(&Scene::default());
    let hash = [7; 32];
    let quad = |x: f32, uv: [f32; 4], color: [f32; 3], tiled: bool| GuiQuad {
        center: Vec2::new(x, 8.0),
        size: Vec2::splat(16.0),
        rotation: 0.0,
        color: [color[0], color[1], color[2], 1.0],
        clip: GuiQuad::NO_CLIP,
        paint: rook_types::Paint::Image(rook_types::ImagePaint {
            hash,
            uv,
            pixelated: true,
            tiled,
        }),
        shape: None,
        gradient: None,
    };
    let mut missing = quad(72.0, [0.0, 0.0, 1.0, 1.0], [1.0; 3], false);
    if let rook_types::Paint::Image(paint) = &mut missing.paint {
        paint.hash = [8; 32];
    }
    let screen = [
        quad(8.0, [0.0, 0.0, 1.0, 1.0], [1.0; 3], false),
        // Mirrored left to right.
        quad(24.0, [1.0, 0.0, 0.0, 1.0], [1.0; 3], false),
        // Two tiles each way: red repeats at the third quarter.
        quad(40.0, [0.0, 0.0, 2.0, 2.0], [1.0; 3], true),
        // White tinted green.
        quad(56.0, [0.5, 0.5, 1.0, 1.0], [0.0, 1.0, 0.0], false),
        missing,
    ];
    let images = [(ImageHash(hash), image())].into_iter().collect();
    r.set_gui(&Arc::from(screen), &[], &images);
    r.finish_uploads();
    let camera = Camera::look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO);
    let (w, h) = (80, 16);
    let pixels = r.render_to_image(w, h, &camera).unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    assert_eq!(at(3, 3), [255, 0, 0]);
    assert_eq!(at(12, 12), [255, 255, 255]);
    assert_eq!(at(19, 3), [0, 255, 0]);
    assert_eq!(at(28, 3), [255, 0, 0]);
    assert_eq!(at(33, 1), [255, 0, 0]);
    assert_eq!(at(41, 1), [255, 0, 0]);
    assert_eq!(at(37, 1), [0, 255, 0]);
    assert_eq!(at(56, 8), [0, 255, 0]);
    assert_ne!(at(72, 8), [255, 0, 0]);
}

/// Screen GUI quads are cut to their shapes (rounded corners, a stroke's
/// band), colored along their gradients, and groups draw their quads into
/// a texture laid over as one, tinted.
#[test]
fn gui_shapes_gradients_and_groups_draw() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    r.set_scene(&Scene::default());
    let fill = |center: Vec2, size: Vec2, color: [f32; 4]| GuiQuad {
        center,
        size,
        rotation: 0.0,
        color,
        clip: GuiQuad::NO_CLIP,
        paint: rook_types::Paint::Solid,
        shape: None,
        gradient: None,
    };
    let white = [1.0; 4];
    let shape = |center: Vec2, radius: f32, ring: f32| rook_types::GuiShape {
        center,
        size: Vec2::splat(16.0),
        radii: [radius; 4],
        ring,
        bevel: false,
    };
    let rounded = GuiQuad {
        shape: Some(shape(Vec2::splat(8.0), 8.0, 0.0)),
        ..fill(Vec2::splat(8.0), Vec2::splat(16.0), white)
    };
    let band = GuiQuad {
        shape: Some(shape(Vec2::new(24.0, 8.0), 0.0, 3.0)),
        ..fill(Vec2::new(24.0, 8.0), Vec2::splat(16.0), white)
    };
    let mut texels = [[0u8; 4]; rook_types::GuiRamp::TEXELS];
    for (i, t) in texels.iter_mut().enumerate() {
        *t = [i as u8, i as u8, i as u8, 255];
    }
    let ramp = std::sync::Arc::new(rook_types::GuiRamp { key: 1, texels });
    let graded = GuiQuad {
        gradient: Some(rook_types::GuiGradient {
            ramp,
            origin: Vec2::new(40.0, 8.0),
            step: Vec2::new(1.0 / 16.0, 0.0),
        }),
        ..fill(Vec2::new(40.0, 8.0), Vec2::splat(16.0), white)
    };
    let group = rook_types::GuiGroup {
        key: 2,
        size: [16, 16],
        quads: vec![
            fill(Vec2::splat(8.0), Vec2::splat(16.0), [1.0, 0.0, 0.0, 1.0]),
            fill(
                Vec2::new(12.0, 8.0),
                Vec2::new(8.0, 16.0),
                [1.0, 1.0, 0.0, 1.0],
            ),
        ],
    };
    let grouped = GuiQuad {
        paint: rook_types::Paint::Group(std::sync::Arc::new(group)),
        ..fill(
            Vec2::new(56.0, 8.0),
            Vec2::splat(16.0),
            [0.0, 1.0, 1.0, 1.0],
        )
    };
    let screen = Arc::from([rounded, band, graded, grouped]);
    r.set_gui(&screen, &[], &Default::default());
    let camera = Camera::look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO);
    let (w, h) = (64, 16);
    let pixels = r.render_to_image(w, h, &camera).unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let sky = at(0, 0);
    assert_ne!(sky, [255; 3]);
    assert_eq!(at(8, 8), [255; 3]);
    assert_eq!(at(8, 1), [255; 3]);
    assert_eq!(at(0, 15), sky);
    assert_eq!(at(17, 0), [255; 3]);
    assert_eq!(at(24, 8), sky);
    // Pixel 39's middle is 0.5 + (39.5 - 40) / 16 along.
    assert!(at(39, 8)[0].abs_diff(120) <= 1, "{:?}", at(39, 8));
    assert!(at(32, 8)[0] < 20 && at(47, 8)[0] > 235);
    // Red, then yellow, both tinted cyan.
    assert_eq!(at(50, 8), [0, 0, 0]);
    assert_eq!(at(62, 8), [0, 255, 0]);
}
