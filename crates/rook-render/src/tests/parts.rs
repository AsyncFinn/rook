use super::*;

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
    incremental.take_images([(hash, image())]);
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

    fresh.take_images([(hash, image())]);
    fresh.set_scene(&scene);
    fresh.finish_uploads();
    assert_eq!(
        incremental.render_to_image(64, 32, &camera).unwrap(),
        fresh.render_to_image(64, 32, &camera).unwrap()
    );
}

/// Parts that move after they've been drawn are drawn where they moved
/// to: their instances go up on their own, not with the rest of the
/// buffer (`batch/scatter.rs`).
#[test]
fn moved_parts_draw_where_they_moved() {
    let (Ok(mut moved), Ok(mut fresh)) = (Renderer::headless(), Renderer::headless()) else {
        return;
    };
    let mut scene = Scene {
        parts: (0..40)
            .map(|i| part(PartShape::Block, i as f32 - 20.0, [1.0, 0.0, 0.0], 0.0))
            .collect(),
        ..Scene::default()
    };
    moved.set_scene(&scene);
    let camera = Camera::look_at(Vec3::new(0.0, 3.0, 30.0), Vec3::ZERO);
    moved.render_to_image(64, 32, &camera).unwrap();
    for i in [3, 17, 30] {
        scene.parts[i].position.y += 2.0;
        scene.parts[i].rotation = Mat3::from_rotation_z(0.4);
    }
    moved.move_parts(&scene, &[3, 17, 30]);
    fresh.set_scene(&scene);
    assert_eq!(
        moved.render_to_image(64, 32, &camera).unwrap(),
        fresh.render_to_image(64, 32, &camera).unwrap()
    );
}

/// A part in a ViewportFrame's world is drawn only in that frame's
/// picture, never in Workspace's view, whether it starts there or moves
/// there (opaque and transparent alike).
#[test]
fn parts_in_a_frames_world_stay_out_of_workspaces_view() {
    let (Ok(mut moved), Ok(mut fresh)) = (Renderer::headless(), Renderer::headless()) else {
        return;
    };
    let framed = |mut p: PartInstance| {
        p.space = Space::Viewport(1);
        p
    };
    let mut scene = Scene {
        parts: vec![
            part(PartShape::Block, -2.0, [1.0, 0.0, 0.0], 0.0),
            part(PartShape::Block, 2.0, [0.0, 1.0, 0.0], 0.5),
            framed(part(PartShape::Ball, 0.0, [0.0, 0.0, 1.0], 0.0)),
        ],
        ..Scene::default()
    };
    let camera = Camera::look_at(Vec3::new(0.0, 3.0, 12.0), Vec3::ZERO);
    let empty = fresh.render_to_image(32, 16, &camera).unwrap();
    fresh.set_scene(&scene);
    assert_eq!(fresh.render_to_image(32, 16, &camera).unwrap(), {
        let mut only = scene.clone();
        only.parts.truncate(2);
        moved.set_scene(&only);
        moved.render_to_image(32, 16, &camera).unwrap()
    });
    moved.set_scene(&scene);
    moved.render_to_image(32, 16, &camera).unwrap();
    scene.parts[0] = framed(scene.parts[0]);
    scene.parts[1] = framed(scene.parts[1]);
    moved.update_parts(&scene, &[0, 1]);
    assert_eq!(moved.render_to_image(32, 16, &camera).unwrap(), empty);
}

/// Terrain's meshes are drawn the frame they're placed, never waiting on
/// the upload budget behind other files: they replace chunks the scene
/// stopped drawing that same frame, so a wait left a hole.
#[test]
fn terrain_meshes_draw_at_once() {
    use rook_content::decode::mesh::{Mesh, TerrainMapping};
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let mesh = |terrain| Mesh {
        positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        normals: vec![[0.0, 1.0, 0.0]; 3],
        uvs: vec![[0.0; 2]; 3],
        indices: vec![0, 2, 1],
        terrain,
        ..Mesh::default()
    };
    let (ground, file) = (MeshHash([1; 32]), MeshHash([2; 32]));
    let at =
        |hash| PartInstance::plain(DrawShape::File(hash), Vec3::ZERO, Mat3::IDENTITY, Vec3::ONE);
    let mut scene = Scene {
        parts: vec![at(ground)],
        meshes: FxHashMap::from_iter([
            (ground, Arc::new(mesh(Some(TerrainMapping::Planes)))),
            (file, Arc::new(mesh(None))),
        ]),
        ..Scene::default()
    };
    r.set_scene(&scene);
    assert!(!r.is_uploading());
    scene.parts.push(at(file));
    r.update_parts(&scene, &[1]);
    assert!(r.is_uploading());
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
        lighting: Lighting {
            ambient: [1.0; 3],
            outdoor_ambient: [0.0; 3],
            sun_color: [0.0; 3],
            ..Lighting::default()
        },
        ..Scene::default()
    };
    r.take_images([(hash, image())]);
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

/// An image wins over another part's face lying in its plane (Studio:
/// a plate flush against a decal's face), but a part just above it hides
/// it, even seen at a low angle: a floor's Texture 0.05 studs under a
/// grating at the top of a lift showed through it.
#[test]
fn images_win_their_plane_but_not_a_part_above() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let hash = ImageHash([2; 32]);
    let red = Arc::new(Image {
        mips: rook_content::decode::image::mips([[255, 0, 0, 255]; 4].concat(), 2, 2),
        ..(*image()).clone()
    });
    let mut floor = part(PartShape::Block, 0.0, [0.5; 3], 0.0);
    floor.size = Vec3::new(200.0, 0.1, 200.0);
    floor.position.y = -0.05;
    let texture = PartInstance {
        layer: Layer::Image {
            image: hash,
            placement: ImagePlacement::Face(NormalId::Top),
            z: 1,
            on_transparent: false,
        },
        color: [1.0; 3],
        ..floor
    };
    let colors = |plate_top: f32, r: &mut Renderer| {
        let mut plate = part(PartShape::Block, 0.0, [0.0, 0.0, 1.0], 0.0);
        plate.size = Vec3::new(200.0, 0.05, 200.0);
        plate.position.y = plate_top - 0.025;
        let scene = Scene {
            parts: vec![floor, texture, plate],
            lighting: Lighting {
                ambient: [1.0; 3],
                outdoor_ambient: [0.0; 3],
                sun_color: [0.0; 3],
                ..Lighting::default()
            },
            ..Scene::default()
        };
        r.take_images([(hash, red.clone())]);
        r.set_scene(&scene);
        // 19 studs up, looking 15 degrees down, as at the lift.
        let camera = Camera {
            fov_y: 30.0,
            ..Camera::look_at(Vec3::new(0.0, 19.0, 70.9), Vec3::ZERO)
        };
        let (w, h) = (256, 256);
        r.render_to_image(w, h, &camera).unwrap();
        let pixels = r.render_to_image(w, h, &camera).unwrap();
        let i = ((h / 2 * w + w / 2) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]].map(i32::from)
    };
    let flush = colors(0.0, &mut r);
    assert!(flush[0] > flush[2] + 100, "flush: {flush:?}");
    let above = colors(0.05, &mut r);
    assert!(above[2] > above[0] + 100, "0.05 above: {above:?}");
}

/// Parts added past what the instance pool holds grow it, and the new
/// buffer takes the old one's contents on the GPU: only the batch that grew
/// goes up, not every instance, and it draws what a fresh scene draws.
#[test]
fn a_growing_instance_pool_uploads_only_what_changed() {
    use rook_types::counters::{Counter, take_local};
    let (Ok(mut grown), Ok(mut fresh)) = (Renderer::headless(), Renderer::headless()) else {
        return;
    };
    let mut scene = Scene::default();
    for i in 0..20_000 {
        let mut p = part(PartShape::Block, (i % 200) as f32 * 3.0, [0.8; 3], 0.0);
        p.position.z = -((i / 200) as f32) * 3.0;
        scene.parts.push(p);
    }
    let camera = Camera::look_at(Vec3::new(300.0, 80.0, 60.0), Vec3::new(300.0, 0.0, -150.0));
    grown.set_scene(&scene);
    grown.render_to_image(32, 32, &camera).unwrap();
    let mut slots = take_local()[Counter::RenderSlots];
    // Balls, a few at a time, until the pool grows.
    let mut image = None;
    for round in 0..200 {
        let start = scene.parts.len();
        for i in 0..500 {
            let mut p = part(
                PartShape::Ball,
                (round * 500 + i) as f32 * 0.01,
                [0.2, 0.4, 0.9],
                0.0,
            );
            p.position.y = 4.0;
            scene.parts.push(p);
        }
        let added: Vec<usize> = (start..scene.parts.len()).collect();
        grown.update_parts(&scene, &added);
        let drawn = grown.render_to_image(32, 32, &camera).unwrap();
        let counts = take_local();
        let now = counts[Counter::RenderSlots];
        if now > slots {
            let uploaded = counts[Counter::RenderSlotsUploaded];
            assert!(uploaded < now / 4, "uploaded {uploaded} of {now} slots");
            image = Some(drawn);
            break;
        }
        slots = now;
    }
    let image = image.expect("the pool grew");
    fresh.set_scene(&scene);
    assert_eq!(image, fresh.render_to_image(32, 32, &camera).unwrap());
}

/// Frames that can't be drawn (a minimized window) still take the scene's
/// changes; what they write and the light buffers they replace are let go,
/// so GPU memory stays as it was however long the window stays down.
#[test]
fn undrawn_frames_hold_no_more_gpu_memory() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let lamp = LocalLight {
        position: Vec3::new(0.0, 3.0, 0.0),
        shape: LightShape::Point,
        color: [1.0, 0.6, 0.2],
        brightness: 1.0,
        range: 16.0,
        shadows: false,
        part: (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
    };
    let mut scene = Scene {
        parts: vec![part(PartShape::Block, 0.0, [0.8; 3], 0.0)],
        lights: vec![lamp; 2000],
        ..Scene::default()
    };
    r.set_scene(&scene);
    let camera = Camera::look_at(Vec3::new(0.0, 40.0, 60.0), Vec3::ZERO);
    r.render_to_image(16, 16, &camera).unwrap();
    let all: Vec<usize> = (0..scene.lights.len()).collect();
    let mut held = |frames: usize, scene: &mut Scene| {
        for _ in 0..frames {
            // Blinking on: the light buffers are built again.
            scene.lights[0].brightness = 1.0 - scene.lights[0].brightness;
            r.update_lights(scene, &all);
            r.submit_undrawn();
        }
        r.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        r.device
            .generate_allocator_report()
            .map(|m| m.total_allocated_bytes)
    };
    let Some(settled) = held(20, &mut scene) else {
        return;
    };
    let later = held(200, &mut scene).expect("reported before");
    assert!(later <= settled, "{settled} bytes, then {later}");
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
    r.take_images([(picture, image())]);
    r.set_scene(&Scene {
        parts: vec![base, decal],
        meshes: FxHashMap::from_iter([(mesh, Arc::new(two))]),
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
    let (scene, images) = busy_scene();
    let camera = Camera::look_at(Vec3::new(0.0, 4.0, 14.0), Vec3::ZERO);
    let mut frames = Vec::new();
    for r in [&mut fast, &mut plain] {
        r.take_images(images.clone());
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
    let (mut scene, images) = busy_scene();
    let camera = Camera::look_at(Vec3::new(0.0, 4.0, 14.0), Vec3::ZERO);
    r.take_images(images.clone());
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
    fresh.take_images(images);
    fresh.set_scene(&scene);
    fresh.finish_uploads();
    assert_eq!(
        r.render_to_image(96, 64, &camera).unwrap(),
        fresh.render_to_image(96, 64, &camera).unwrap()
    );
}

/// A Highlight fills its part in exactly its color over the finished
/// frame and outlines it, through a wall when on top; occluded, the wall
/// hides both. Its look changes without the part changing.
#[test]
fn highlights_fill_and_outline_through_walls_when_on_top() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let highlight = |on_top| Highlight {
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
                highlight: Some(1),
                ..part(PartShape::Block, 0.0, [1.0, 0.0, 0.0], 0.0)
            },
            wall,
        ],
        highlights: [(1, highlight(true))].into_iter().collect(),
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

    scene.highlights.insert(1, highlight(false));
    r.update_highlights(&scene);
    let pixels = r.render_to_image(w, h, &camera).unwrap();
    assert!(!(0..w).any(|x| at(&pixels, x, 32) == [0, 0, 255]));
    // The wall, as with no highlight at all.
    scene.highlights.clear();
    r.update_highlights(&scene);
    let plain = r.render_to_image(w, h, &camera).unwrap();
    assert_eq!(at(&pixels, 32, 32), at(&plain, 32, 32));
}

/// A mesh seen from behind draws only if `DoubleSided`, and then lit as if
/// it faced the viewer: a sun on the viewer's side lights it, one behind it
/// doesn't.
#[test]
fn double_sided_backs_draw_lit_towards_the_viewer() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let behind = Camera::look_at(Vec3::new(0.0, 0.0, -8.0), Vec3::ZERO);
    let toward_viewer = Vec3::new(0.0, 0.3, -1.0);
    let away = Vec3::new(0.0, 0.3, 1.0);
    let single = middle(&mut r, &square_scene(false, toward_viewer), &behind);
    assert!(single[2] > single[0], "the sky shows through: {single:?}");
    let lit = middle(&mut r, &square_scene(true, toward_viewer), &behind);
    let unlit = middle(&mut r, &square_scene(true, away), &behind);
    assert!(lit[0] > lit[2], "{lit:?}");
    assert!(lit[0] > unlit[0] + 40, "{lit:?} {unlit:?}");
}
