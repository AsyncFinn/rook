use rustc_hash::FxHashMap;
use std::sync::Arc;

use glam::Mat3;
use rook_types::{NormalDetail, SurfacePattern};

use super::*;

mod gui;

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
        images: FxHashMap::from_iter([(hash, image())]),
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
        images: FxHashMap::from_iter([(hash, image())]),
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
            images: FxHashMap::from_iter([(hash, red.clone())]),
            lighting: Lighting {
                ambient: [1.0; 3],
                outdoor_ambient: [0.0; 3],
                sun_color: [0.0; 3],
                ..Lighting::default()
            },
            ..Scene::default()
        };
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
    r.update_lighting(&scene);
    r.set_time(3.0);
    assert!((r.water_phase - 10.0).abs() < 1e-4);
    scene.terrain.wave_speed = 4.0;
    r.update_lighting(&scene);
    r.set_time(3.5);
    assert!((r.water_phase - 12.0).abs() < 1e-4);
    r.set_time(0.0);
    assert!((r.water_phase - 12.0).abs() < 1e-4);
}

/// A part behind the camera, culled from the main pass, still shadows the
/// ground in view: the shadow pass draws what the sun sees.
/// The sky's tint for the time of day reaches all the sky does: at night
/// (tint 0) the sky draws black, and a part lit only by it and reflecting
/// it whole draws black too; by day both show.
#[test]
fn a_night_sky_draws_lights_and_reflects_nothing() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let mut mirror = part(PartShape::Block, 0.0, [0.0; 3], 0.0);
    mirror.size = Vec3::new(20.0, 1.0, 20.0);
    mirror.reflectance = 1.0;
    let camera = Camera::look_at(Vec3::new(0.0, 6.0, 20.0), Vec3::new(0.0, 0.0, 0.0));
    for (tint, lit) in [([1.0; 3], true), ([0.0; 3], false)] {
        r.set_scene(&Scene {
            parts: vec![mirror],
            lighting: Lighting {
                ambient: [0.0; 3],
                outdoor_ambient: [0.0; 3],
                sun_color: [0.0; 3],
                environment_diffuse: 1.0,
                environment_specular: 1.0,
                sky_tint: tint,
                ..Lighting::default()
            },
            ..Scene::default()
        });
        let image = r.render_to_image(32, 32, &camera).unwrap();
        let at = |x: usize, y: usize| &image[(y * 32 + x) * 4..][..3];
        let (sky, part) = (at(16, 1), at(16, 20));
        assert_eq!(
            sky.iter().any(|&c| c > 0),
            lit,
            "sky {sky:?} at tint {tint:?}"
        );
        assert_eq!(
            part.iter().any(|&c| c > 0),
            lit,
            "part {part:?} at tint {tint:?}"
        );
    }
}

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

/// Parts and a light that move refill only the light voxels they reach,
/// and those draw what filling them all draws: the sun's shadow, a light
/// with `Shadows` behind a wall, and the sky's light under a roof (the
/// place lit by the sky, then not, as each reaches voxels the others do).
#[test]
fn moved_parts_and_lights_refill_what_a_whole_fill_draws() {
    let (Ok(mut moved), Ok(mut fresh)) = (Renderer::headless(), Renderer::headless()) else {
        return;
    };
    let mut ground = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    ground.position.y = -0.5;
    ground.size = Vec3::new(200.0, 1.0, 200.0);
    let mut roof = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    // Far enough from the light that their voxels' tiles differ.
    roof.position = Vec3::new(-90.0, 12.0, 0.0);
    roof.size = Vec3::new(24.0, 1.0, 24.0);
    let mut wall = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    wall.position = Vec3::new(14.0, 4.0, 0.0);
    wall.size = Vec3::new(1.0, 8.0, 12.0);
    let light = LocalLight {
        position: Vec3::new(20.0, 3.0, 0.0),
        shape: LightShape::Point,
        color: [1.0, 0.6, 0.2],
        brightness: 4.0,
        range: 24.0,
        shadows: true,
        part: (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
    };
    let camera = Camera::look_at(Vec3::new(-25.0, 70.0, -90.0), Vec3::new(-25.0, 0.0, 0.0));
    for diffuse in [1.0, 0.0] {
        let mut scene = Scene {
            parts: vec![ground, roof, wall],
            lights: vec![light],
            lighting: Lighting {
                environment_diffuse: diffuse,
                ..Lighting::default()
            },
            ..Scene::default()
        };
        moved.set_scene(&scene);
        let mut before = moved.render_to_image(64, 64, &camera).unwrap();
        // The parts first, then the light, as each reaches the other's
        // voxels.
        for step in 0..2 {
            if step == 0 {
                scene.parts[1].position.x += 9.0;
                scene.parts[2].position.z += 5.0;
                scene.parts[2].rotation = Mat3::from_rotation_y(0.3);
                moved.move_parts(&scene, &[1, 2]);
            } else {
                scene.lights[0].position.x += 3.0;
                moved.update_lights(&scene, &[0]);
            }
            let after = moved.render_to_image(64, 64, &camera).unwrap();
            fresh.set_scene(&scene);
            assert_ne!(before, after);
            assert_eq!(after, fresh.render_to_image(64, 64, &camera).unwrap());
            before = after;
        }
    }
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

/// A light that turns on, one that comes and one that moves past where the
/// hash placed it each build the hash again, and refill only the tiles
/// they light, not the whole volume; every voxel still comes out as
/// filling them all.
#[test]
fn lights_that_change_the_hash_refill_only_what_they_light() {
    let (Ok(mut moved), Ok(mut fresh)) = (Renderer::headless(), Renderer::headless()) else {
        return;
    };
    let mut ground = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    ground.position.y = -0.5;
    ground.size = Vec3::new(200.0, 1.0, 200.0);
    let lamp = LocalLight {
        position: Vec3::new(20.0, 3.0, 0.0),
        shape: LightShape::Point,
        color: [1.0, 0.6, 0.2],
        brightness: 0.0,
        range: 16.0,
        shadows: false,
        part: (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
    };
    let mut scene = Scene {
        parts: vec![ground],
        lights: vec![lamp],
        ..Scene::default()
    };
    let camera = Camera::look_at(Vec3::new(0.0, 40.0, 60.0), Vec3::ZERO);
    moved.set_scene(&scene);
    moved.render_to_image(16, 16, &camera).unwrap();
    let whole = 16 * 16 * 64;
    for step in 0..4 {
        match step {
            // Turned on: it reaches further than it did at brightness 0.
            0 => scene.lights[0].brightness = 4.0,
            1 => scene.lights.push(LocalLight {
                position: Vec3::new(-30.0, 3.0, 10.0),
                ..scene.lights[0]
            }),
            2 => scene.lights[0].position.x += 12.0,
            _ => _ = scene.lights.pop(),
        }
        let changed: Vec<usize> = (0..scene.lights.len()).collect();
        moved.update_lights(&scene, &changed);
        moved.render_to_image(16, 16, &camera).unwrap();
        let filled = moved.lights.filled_tiles();
        assert!(
            filled > 0 && filled < whole / 4,
            "step {step}: filled {filled} tiles of {whole}"
        );
        fresh.set_scene(&scene);
        fresh.render_to_image(16, 16, &camera).unwrap();
        assert!(moved.light_voxels() == fresh.light_voxels(), "step {step}");
    }
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

/// An off light (black: Brightness 0) on a part that creeps by float noise
/// every frame refills no voxels, however far it goes; when it lights up,
/// every voxel comes out as filling them all.
#[test]
fn an_off_light_that_moves_refills_nothing() {
    let (Ok(mut moved), Ok(mut fresh)) = (Renderer::headless(), Renderer::headless()) else {
        return;
    };
    let mut ground = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    ground.position.y = -0.5;
    ground.size = Vec3::new(200.0, 1.0, 200.0);
    let mut scene = Scene {
        parts: vec![ground],
        lights: vec![LocalLight {
            position: Vec3::new(20.0, 3.0, 0.0),
            shape: LightShape::Point,
            color: [1.0, 0.6, 0.2],
            brightness: 0.0,
            range: 16.0,
            shadows: true,
            part: (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        }],
        ..Scene::default()
    };
    let camera = Camera::look_at(Vec3::new(0.0, 40.0, 60.0), Vec3::ZERO);
    moved.set_scene(&scene);
    moved.render_to_image(16, 16, &camera).unwrap();
    for step in [3e-5, 3e-5, 20.0] {
        scene.lights[0].position.x += step;
        moved.update_lights(&scene, &[0]);
        moved.render_to_image(16, 16, &camera).unwrap();
        assert_eq!(moved.lights.filled_tiles(), 0, "moved {step}");
    }
    scene.lights[0].brightness = 4.0;
    moved.update_lights(&scene, &[0]);
    moved.render_to_image(16, 16, &camera).unwrap();
    assert!(moved.lights.filled_tiles() > 0);
    fresh.set_scene(&scene);
    fresh.render_to_image(16, 16, &camera).unwrap();
    assert!(moved.light_voxels() == fresh.light_voxels());
}

/// A part moving under a light with `Shadows` refills the voxels behind
/// it seen from the light, layer by layer below it, not all the light
/// reaches; every voxel still comes out as filling them all.
#[test]
fn a_part_moving_under_a_shadowed_light_refills_its_shadow() {
    let (Ok(mut moved), Ok(mut fresh)) = (Renderer::headless(), Renderer::headless()) else {
        return;
    };
    let mut ground = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    ground.position.y = -0.5;
    ground.size = Vec3::new(200.0, 1.0, 200.0);
    let mut crate_ = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    crate_.position = Vec3::new(20.0, 24.0, 6.0);
    crate_.size = Vec3::splat(2.0);
    let mut scene = Scene {
        parts: vec![ground, crate_],
        lights: vec![LocalLight {
            position: Vec3::new(0.0, 40.0, 0.0),
            shape: LightShape::Point,
            color: [1.0; 3],
            brightness: 2.0,
            range: 60.0,
            shadows: true,
            part: (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        }],
        lighting: Lighting {
            environment_diffuse: 0.0,
            prioritize_quality: true,
            ..Lighting::default()
        },
        ..Scene::default()
    };
    let camera = Camera::look_at(Vec3::new(0.0, 60.0, 80.0), Vec3::ZERO);
    moved.set_scene(&scene);
    moved.render_to_image(16, 16, &camera).unwrap();
    scene.parts[1].rotation = Mat3::from_rotation_y(0.7);
    moved.move_parts(&scene, &[1]);
    moved.render_to_image(16, 16, &camera).unwrap();
    // All the light reaches is 112 tiles here; the crate's shadow is two
    // by two tiles in each of the 13 layers from below it to the light's
    // lowest.
    let filled = moved.lights.filled_tiles();
    assert!(filled > 0 && filled <= 2 * 2 * 13, "filled {filled} tiles");
    fresh.set_scene(&scene);
    fresh.render_to_image(16, 16, &camera).unwrap();
    assert!(moved.light_voxels() == fresh.light_voxels());
}

/// A part moving high under open sky (a fan turning, say) refills the
/// light voxels in its own columns, not the cone below it that the sky's
/// light spreads through under roofs: with nothing blocked above them,
/// those voxels are lit by all of the sky whatever moved. What it draws is
/// still what filling them all draws.
#[test]
fn a_part_moving_under_open_sky_refills_only_its_columns() {
    let (Ok(mut moved), Ok(mut fresh)) = (Renderer::headless(), Renderer::headless()) else {
        return;
    };
    let mut roof = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    roof.position = Vec3::new(-60.0, 10.0, 0.0);
    roof.size = Vec3::new(24.0, 1.0, 24.0);
    let mut blade = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    blade.position = Vec3::new(40.0, 90.0, 0.0);
    blade.size = Vec3::new(6.0, 1.0, 1.0);
    let mut scene = Scene {
        parts: vec![roof, blade],
        lighting: Lighting {
            environment_diffuse: 1.0,
            prioritize_quality: true,
            ..Lighting::default()
        },
        ..Scene::default()
    };
    let camera = Camera::look_at(Vec3::new(0.0, 60.0, 80.0), Vec3::new(0.0, 0.0, 0.0));
    moved.set_scene(&scene);
    moved.render_to_image(64, 64, &camera).unwrap();
    scene.parts[1].rotation = Mat3::from_rotation_y(0.7);
    moved.move_parts(&scene, &[1]);
    let after = moved.render_to_image(64, 64, &camera).unwrap();
    // Its tile column (two tiles at most each way) in every layer below it;
    // the cone it widens into reaches tens of tiles a layer at the bottom.
    let filled = moved.lights.filled_tiles();
    assert!(filled > 0 && filled <= 4 * 64, "filled {filled} tiles");
    fresh.set_scene(&scene);
    assert_eq!(after, fresh.render_to_image(64, 64, &camera).unwrap());
}

/// A part moving under a low sun the voxels shadow refills the voxels
/// along its shadow's way down, layer by layer, not the box around the
/// whole slanted path; every voxel still comes out as filling them all.
#[test]
fn a_part_moving_under_a_low_sun_refills_its_shadows_path() {
    let (Ok(mut moved), Ok(mut fresh)) = (Renderer::headless(), Renderer::headless()) else {
        return;
    };
    let mut ground = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    ground.position.y = -0.5;
    ground.size = Vec3::new(400.0, 1.0, 400.0);
    let mut blade = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    blade.position = Vec3::new(0.0, 100.0, 0.0);
    blade.size = Vec3::new(6.0, 1.0, 1.0);
    let mut scene = Scene {
        parts: vec![ground, blade],
        lighting: Lighting {
            sun_direction: Vec3::new(1.0, 0.6, 0.4).normalize(),
            ..Lighting::default()
        },
        ..Scene::default()
    };
    let camera = Camera::look_at(Vec3::new(0.0, 60.0, 80.0), Vec3::ZERO);
    moved.set_scene(&scene);
    moved.render_to_image(16, 16, &camera).unwrap();
    scene.parts[1].rotation = Mat3::from_rotation_y(0.7);
    moved.move_parts(&scene, &[1]);
    moved.render_to_image(16, 16, &camera).unwrap();
    // A tile or two each way per layer below it; the box around the
    // slanted path is several tiles wide along x and z in every layer.
    let filled = moved.lights.filled_tiles();
    assert!(filled > 0 && filled <= 9 * 64, "filled {filled} tiles");
    fresh.set_scene(&scene);
    fresh.render_to_image(16, 16, &camera).unwrap();
    assert!(moved.light_voxels() == fresh.light_voxels());
}

/// A camera that moves and turns far enough for the light voxels to follow
/// fills only what the volume came onto and what that reaches, and every
/// voxel comes out as filling them all where it went: the sky's light and
/// the sun under roofs that straddle the faces it crossed, and a light
/// with `Shadows` (with the sky and the sun in the voxels, then neither).
#[test]
fn a_moved_light_volume_fills_what_it_came_onto() {
    let (Ok(mut moved), Ok(mut fresh)) = (Renderer::headless(), Renderer::headless()) else {
        return;
    };
    let mut parts = Vec::new();
    let mut ground = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
    ground.position.y = -0.5;
    ground.size = Vec3::new(1600.0, 1.0, 1600.0);
    parts.push(ground);
    // Roofs everywhere the volume's faces may fall.
    for x in -10..=10 {
        for z in -10..=10 {
            let mut roof = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
            roof.position = Vec3::new(x as f32 * 76.0, 14.0, z as f32 * 76.0);
            roof.size = Vec3::new(30.0, 1.0, 30.0);
            parts.push(roof);
        }
    }
    let light = LocalLight {
        position: Vec3::new(120.0, 6.0, -30.0),
        shape: LightShape::Point,
        color: [1.0, 0.6, 0.2],
        brightness: 4.0,
        range: 30.0,
        shadows: true,
        part: (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
    };
    let here = Camera::look_at(Vec3::new(0.0, 40.0, 60.0), Vec3::ZERO);
    let there = Camera::look_at(Vec3::new(150.0, 40.0, 20.0), Vec3::new(200.0, 0.0, -40.0));
    for (diffuse, quality) in [(1.0, false), (0.0, true)] {
        let scene = Scene {
            parts: parts.clone(),
            lights: vec![light],
            lighting: Lighting {
                environment_diffuse: diffuse,
                prioritize_quality: quality,
                sun_direction: Vec3::new(0.4, 1.0, 0.3).normalize(),
                ..Lighting::default()
            },
            ..Scene::default()
        };
        moved.set_scene(&scene);
        moved.render_to_image(16, 16, &here).unwrap();
        // The parts where it went are marked a little each frame first.
        let mut filled = Vec::new();
        for _ in 0..30 {
            moved.render_to_image(16, 16, &there).unwrap();
            filled.push(moved.lights.filled_tiles());
        }
        let whole = 16 * 16 * 64;
        assert!(
            filled.iter().any(|&f| f > 0) && filled.iter().all(|&f| f < whole),
            "filled {filled:?} tiles of {whole}"
        );
        fresh.set_scene(&scene);
        fresh.render_to_image(16, 16, &there).unwrap();
        let [light, sun] = moved.light_voxels();
        let [fresh_light, fresh_sun] = fresh.light_voxels();
        assert!(light == fresh_light, "light voxels (sky {diffuse})");
        assert!(quality || sun == fresh_sun, "sun voxels");
    }
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
        images: FxHashMap::from_iter([(picture, image())]),
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
        images: FxHashMap::from_iter([(a, image()), (b, image())]),
        meshes: FxHashMap::from_iter([(m1, square(2.0)), (m2, square(1.5))]),
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
        images: FxHashMap::from_iter([(hash, Arc::new(map))]),
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
    r.set_scene(&Scene {
        parts: vec![block],
        images: maps.into_iter().collect(),
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
    assert_eq!(at(&pixels, 32, 32), at(&pixels, 4, 4));
    assert!(!(0..w).any(|x| at(&pixels, x, 32) == [0, 0, 255]));
}

/// The middle pixel's channels added up, of a Glass block before a wall of
/// `wall` gray, seen straight on.
fn glass_middle(r: &mut Renderer, glass: Option<PartInstance>, wall: f32) -> u32 {
    let mut back = part(PartShape::Block, 0.0, [wall; 3], 0.0);
    back.material = Material::SmoothPlastic;
    back.position.z = -3.0;
    back.size = Vec3::new(20.0, 20.0, 1.0);
    let mut parts = vec![back];
    parts.extend(glass);
    r.set_scene(&Scene {
        parts,
        ..Scene::default()
    });
    r.finish_uploads();
    let camera = Camera::look_at(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO);
    let pixels = r.render_to_image(16, 16, &camera).unwrap();
    let at = (8 * 16 + 8) * 4;
    pixels[at..at + 3].iter().map(|&c| u32::from(c)).sum()
}

/// `glass_middle` over a white wall with `behind` between.
fn glass_middle_behind(r: &mut Renderer, glass: PartInstance, behind: PartInstance) -> u32 {
    let mut back = part(PartShape::Block, 0.0, [1.0; 3], 0.0);
    back.material = Material::SmoothPlastic;
    back.position.z = -3.0;
    back.size = Vec3::new(20.0, 20.0, 1.0);
    r.set_scene(&Scene {
        parts: vec![back, behind, glass],
        ..Scene::default()
    });
    r.finish_uploads();
    let camera = Camera::look_at(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO);
    let pixels = r.render_to_image(16, 16, &camera).unwrap();
    let at = (8 * 16 + 8) * 4;
    pixels[at..at + 3].iter().map(|&c| u32::from(c)).sum()
}

fn glass(color: [f32; 3], transparency: f32, reflectance: f32) -> PartInstance {
    let mut block = part(PartShape::Block, 0.0, color, transparency);
    block.material = Material::Glass;
    block.reflectance = reflectance;
    block
}

/// Reflectance 1 makes Glass a mirror whatever its Color; at 0 black Glass
/// is black face-on, sky and all.
#[test]
fn glass_mirrors_by_reflectance_alone() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let black = glass_middle(&mut r, Some(glass([0.0; 3], 0.0, 1.0)), 0.0);
    let white = glass_middle(&mut r, Some(glass([1.0; 3], 0.0, 1.0)), 0.0);
    assert!(black.abs_diff(white) < 30, "{black} against {white}");
    assert!(black > 100, "{black}");
    let plain = glass_middle(&mut r, Some(glass([0.0; 3], 0.0, 0.0)), 0.0);
    assert!(plain < 40, "{plain}");
}

/// Past the rim's angle only the outline turns to sky: black Glass tilted
/// away draws what it mirrors, facing the camera nothing.
#[test]
fn glass_rim_grows_towards_the_outline() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let mut tilted = glass([0.0; 3], 0.0, 0.0);
    tilted.rotation = Mat3::from_rotation_y(1.2);
    tilted.size = Vec3::new(4.0, 4.0, 0.5);
    let rim = glass_middle(&mut r, Some(tilted), 0.0);
    let face = glass_middle(&mut r, Some(glass([0.0; 3], 0.0, 0.0)), 0.0);
    assert!(rim > face + 60, "{rim} against {face}");
}

/// A see-through part behind see-through Glass doesn't show, nor does
/// Glass behind Glass: only what's opaque or water is behind it.
#[test]
fn glass_hides_see_through_parts_behind_it() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let front = glass([1.0; 3], 0.7, 0.0);
    let bare = glass_middle(&mut r, Some(front), 1.0);
    for material in [Material::SmoothPlastic, Material::Glass] {
        let mut behind = part(PartShape::Block, 0.0, [1.0, 0.0, 0.0], 0.5);
        behind.material = material;
        behind.position.z = -1.5;
        behind.size = Vec3::new(2.0, 2.0, 0.2);
        let hidden = glass_middle_behind(&mut r, front, behind);
        assert!(
            bare.abs_diff(hidden) < 6,
            "{material:?}: {hidden} against {bare}"
        );
    }
}

/// See-through Glass filters what's behind it by its Color, fully from
/// Transparency 0.5 down, and its surface goes by Transparency 0.5.
#[test]
fn glass_filters_what_is_behind_it() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let wall = glass_middle(&mut r, None, 0.5);
    let black_half = glass_middle(&mut r, Some(glass([0.0; 3], 0.5, 0.0)), 0.5);
    assert!(black_half < 10, "{black_half}");
    let black_most = glass_middle(&mut r, Some(glass([0.0; 3], 0.9, 0.0)), 0.5);
    assert!(black_most * 2 > wall, "{black_most} of {wall}");
    // White filters nothing: at 0.25, half of what's behind still shows
    // through its surface, beside the surface's own light.
    let over_gray = glass_middle(&mut r, Some(glass([1.0; 3], 0.25, 0.0)), 0.5);
    let over_black = glass_middle(&mut r, Some(glass([1.0; 3], 0.25, 0.0)), 0.0);
    let shown = over_gray - over_black;
    assert!(shown * 3 > wall, "{shown} of {wall}");
}

/// A red square 4 studs wide facing +Z, double-sided or not, in a scene of
/// a sun at `sun` alone.
fn square_scene(double_sided: bool, sun: Vec3) -> Scene {
    let mesh = MeshHash([9; 32]);
    Scene {
        parts: vec![PartInstance {
            color: [0.8, 0.1, 0.1],
            double_sided,
            ..PartInstance::plain(DrawShape::File(mesh), Vec3::ZERO, Mat3::IDENTITY, Vec3::ONE)
        }],
        meshes: FxHashMap::from_iter([(mesh, square(4.0))]),
        lighting: Lighting {
            sun_direction: sun.normalize(),
            ..Lighting::default()
        },
        ..Scene::default()
    }
}

/// The pixel at the middle of a 32 x 32 render.
fn middle(r: &mut Renderer, scene: &Scene, camera: &Camera) -> [u8; 3] {
    r.set_scene(scene);
    r.render_to_image(32, 32, camera).unwrap();
    let pixels = r.render_to_image(32, 32, camera).unwrap();
    let i = (16 * 32 + 16) * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
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

/// A square facing down casts no shadow from an overhead sun, as its only
/// faces point away from it; `DoubleSided` gives it faces towards the sun.
#[test]
fn double_sided_meshes_shadow_from_their_back() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let ground_at = |r: &mut Renderer, double_sided: bool| {
        let mut scene = square_scene(double_sided, Vec3::Y);
        // Turned to face down, over a ground.
        scene.parts[0].rotation = Mat3::from_rotation_x(std::f32::consts::FRAC_PI_2);
        scene.parts[0].position.y = 4.0;
        scene.lighting.prioritize_quality = true;
        let mut ground = part(PartShape::Block, 0.0, [0.8; 3], 0.0);
        ground.position.y = -0.5;
        ground.size = Vec3::new(100.0, 1.0, 100.0);
        scene.parts.push(ground);
        let camera = Camera {
            fov_y: 20.0,
            ..Camera::look_at(Vec3::new(9.0, 1.0, 0.0), Vec3::ZERO)
        };
        middle(r, &scene, &camera)
    };
    let single = ground_at(&mut r, false);
    let double = ground_at(&mut r, true);
    assert!(single[0] > double[0] + 30, "{single:?} {double:?}");
}

/// See-through Glass filters what's behind it from the front; the back of a
/// `DoubleSided` mesh shows next to nothing at Transparency 0.5, neither
/// filter nor rim.
#[test]
fn double_sided_glass_filters_from_the_front_only() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let mut scene = square_scene(true, Vec3::Y);
    scene.parts[0].material = Material::Glass;
    scene.parts[0].transparency = 0.5;
    scene.parts[0].color = [0.1, 0.2, 0.8];
    let camera = |z: f32| Camera::look_at(Vec3::new(0.0, 0.0, z), Vec3::ZERO);
    let front = middle(&mut r, &scene, &camera(8.0));
    let back = middle(&mut r, &scene, &camera(-8.0));
    scene.parts.clear();
    let bare = middle(&mut r, &scene, &camera(-8.0));
    assert!(back[1] > front[1] + 40, "{back:?} {front:?}");
    for (shown, sky) in back.iter().zip(bare) {
        assert!(shown.abs_diff(sky) < 8, "{back:?} over {bare:?}");
    }
}
