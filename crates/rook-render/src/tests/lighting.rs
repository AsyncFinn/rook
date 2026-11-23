use super::*;

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
    // Per layer below it, the span its cones cross there and their widest
    // footprint each way (the cover's coarsest texel and a half, so six
    // tiles across at most); the box around the slanted path is wider
    // still in every layer.
    let filled = moved.lights.filled_tiles();
    assert!(filled > 0 && filled <= 36 * 64, "filled {filled} tiles");
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
        // The parts where it went are marked a little each frame first,
        // as many frames as the machine's speed takes, up to a brick a
        // frame; then it fills.
        let mut filled = Vec::new();
        while filled.last().is_none_or(|&f| f == 0)
            && filled.len() < crate::lighting::local::GpuLights::most_frames_to_move()
        {
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
