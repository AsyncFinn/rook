//! The GUI drawn into the world and on the screen.

use std::sync::Arc;

use glam::Vec2;

use super::*;

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
    r.place_world_gui(
        [
            Some(WorldPlace::Surface {
                origin: Vec3::new(-2.0, 2.0, 0.5),
                right: Vec3::X,
                down: Vec3::NEG_Y,
            }),
            Some(billboard(1.0)),
            Some(billboard(-1.0)),
        ]
        .into_iter()
        .enumerate(),
    );
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

/// World canvases drawn in a row with one image are one draw call where
/// the device draws lists of draws, each still where it is; dropping some
/// and adding one with more quads than the shared buffer first holds
/// moves nothing that stayed.
#[test]
fn world_canvases_in_a_row_draw_in_one_call() {
    use rook_types::counters::{Counter, take_local};
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    r.set_scene(&Scene::default());
    let fill = |color: [f32; 3]| GuiQuad {
        center: Vec2::splat(5.0),
        size: Vec2::splat(10.0),
        rotation: 0.0,
        color: [color[0], color[1], color[2], 1.0],
        clip: GuiQuad::NO_CLIP,
        paint: rook_types::Paint::Solid,
        shape: None,
        gradient: None,
    };
    let canvas = |quads: Vec<GuiQuad>| {
        Arc::new(WorldCanvas {
            quads,
            size: Vec2::splat(10.0),
            light_influence: 0.0,
            brightness: 1.0,
            always_on_top: false,
            max_distance: f32::INFINITY,
            untoned: false,
        })
    };
    let (red, green) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let row: Vec<Arc<WorldCanvas>> = [red, green, red, green]
        .map(|c| canvas(vec![fill(c)]))
        .into();
    // Billboards 6 pixels across, a stud apart: their middles are at
    // pixels 21, 28, 35 and 42.
    let billboard = |x: f32| WorldPlace::Billboard {
        anchor: Vec3::new(x, 0.0, -5.0),
        studs: Vec3::ZERO,
        size_studs: Vec2::ZERO,
        size_pixels: Vec2::splat(6.0),
        shift: Vec2::ZERO,
    };
    let camera = Camera {
        fov_y: 20.0,
        ..Camera::look_at(Vec3::new(0.0, 0.0, 20.0), Vec3::ZERO)
    };
    let (w, h) = (64, 64);
    let draw = |r: &mut Renderer| {
        take_local();
        let pixels = r.render_to_image(w, h, &camera).unwrap();
        let calls = take_local()[Counter::RenderGuiCalls];
        let at = move |x: u32| {
            let i = ((32 * w + x) * 4) as usize;
            [pixels[i], pixels[i + 1], pixels[i + 2]]
        };
        ([21, 28, 35, 42].map(at), calls)
    };
    r.set_gui(&Arc::from([]), &row, &Default::default());
    r.place_world_gui(
        [-1.5, -0.5, 0.5, 1.5]
            .map(|x| Some(billboard(x)))
            .into_iter()
            .enumerate(),
    );
    let (seen, calls) = draw(&mut r);
    let is = |c: [u8; 3], channel: usize| c[channel] > 200 && c[(channel + 1) % 3] < 30;
    assert!(is(seen[0], 0) && is(seen[1], 1), "{seen:?}");
    assert!(is(seen[2], 0) && is(seen[3], 1), "{seen:?}");
    if rook_types::counters::ENABLED {
        assert_eq!(calls, if r.multi_draw { 1 } else { 4 });
    }
    // The greens go, and a blue canvas of 5,000 quads comes.
    let blue = canvas(vec![fill([0.0, 0.0, 1.0]); 5_000]);
    let next = [row[0].clone(), row[2].clone(), blue];
    r.set_gui(&Arc::from([]), &next, &Default::default());
    r.place_world_gui(
        [-1.5, 0.5, 1.5]
            .map(|x| Some(billboard(x)))
            .into_iter()
            .enumerate(),
    );
    let (now, _) = draw(&mut r);
    assert_eq!((now[0], now[2]), (seen[0], seen[2]));
    assert_ne!(now[1], seen[1]);
    assert!(is(now[3], 2), "{now:?}");
}

/// A canvas between two transparent panes it overlaps on the screen is
/// drawn after the far one and before the near one, whatever else it's
/// drawn with: the near pane tints it and the far one is hidden behind
/// it. A canvas beside them overlaps neither and shows its own color.
#[test]
fn world_canvases_sort_among_the_transparent_parts_they_overlap() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let pane = |z: f32, color: [f32; 3]| {
        let mut p = part(PartShape::Block, 0.0, color, 0.5);
        p.position.z = z;
        p.size = Vec3::new(1.0, 1.0, 0.2);
        p
    };
    r.set_scene(&Scene {
        parts: vec![pane(0.0, [1.0, 0.0, 0.0]), pane(-10.0, [0.0, 0.0, 1.0])],
        lighting: Lighting {
            ambient: [1.0; 3],
            outdoor_ambient: [0.0; 3],
            sun_color: [0.0; 3],
            ..Lighting::default()
        },
        ..Scene::default()
    });
    let green = Arc::new(WorldCanvas {
        quads: vec![GuiQuad {
            center: Vec2::splat(5.0),
            size: Vec2::splat(10.0),
            rotation: 0.0,
            color: [0.0, 1.0, 0.0, 1.0],
            clip: GuiQuad::NO_CLIP,
            paint: rook_types::Paint::Solid,
            shape: None,
            gradient: None,
        }],
        size: Vec2::splat(10.0),
        light_influence: 0.0,
        brightness: 1.0,
        always_on_top: false,
        max_distance: f32::INFINITY,
        untoned: false,
    });
    let canvases = [green.clone(), Arc::new((*green).clone())];
    r.set_gui(&Arc::from([]), &canvases, &Default::default());
    // Between the panes, 25 studs out: 6 pixels across at the middle, and
    // 2 studs to the side, around pixel 46.
    let billboard = |x: f32| WorldPlace::Billboard {
        anchor: Vec3::new(x, 0.0, -5.0),
        studs: Vec3::ZERO,
        size_studs: Vec2::ZERO,
        size_pixels: Vec2::splat(6.0),
        shift: Vec2::ZERO,
    };
    r.place_world_gui(
        [0.0, 2.0]
            .map(|x| Some(billboard(x)))
            .into_iter()
            .enumerate(),
    );
    let camera = Camera {
        fov_y: 20.0,
        ..Camera::look_at(Vec3::new(0.0, 0.0, 20.0), Vec3::ZERO)
    };
    let (w, h) = (64, 64);
    let pixels = r.render_to_image(w, h, &camera).unwrap();
    let at = |x: u32| {
        let i = ((32 * w + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let (between, beside) = (at(32), at(46));
    assert!(
        beside[1] > 200 && beside[0] < 30 && beside[2] < 30,
        "{beside:?}"
    );
    // Tinted red by the near pane, nothing of the far one's blue.
    assert!(
        between[0] > 60 && between[1] > 60 && between[2] < 30,
        "{between:?}"
    );
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
    r.set_gui(&Arc::from([Arc::<[GuiQuad]>::from(screen)]), &[], &images);
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
    let screen = Arc::from([Arc::<[GuiQuad]>::from([rounded, band, graded, grouped])]);
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

/// Screen GUI pieces given again are kept, and a changed one draws as a
/// fresh renderer draws the whole list: written alone when it keeps its
/// size, with everything after it when it grows.
#[test]
fn gui_pieces_redraw_as_given() {
    let (Ok(mut kept), Ok(mut fresh)) = (Renderer::headless(), Renderer::headless()) else {
        return;
    };
    let square = |x: f32, color: [f32; 3]| GuiQuad {
        center: Vec2::new(x, 8.0),
        size: Vec2::splat(16.0),
        rotation: 0.0,
        color: [color[0], color[1], color[2], 1.0],
        clip: GuiQuad::NO_CLIP,
        paint: rook_types::Paint::Solid,
        shape: None,
        gradient: None,
    };
    let piece = |quads: Vec<GuiQuad>| -> Arc<[GuiQuad]> { quads.into() };
    let (red, green, blue) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    let a = piece(vec![square(8.0, red)]);
    let b = piece(vec![square(24.0, green)]);
    let c = piece(vec![square(40.0, blue)]);
    let camera = Camera::look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO);
    let draw = |r: &mut Renderer, list: &[Arc<[GuiQuad]>]| {
        r.set_scene(&Scene::default());
        r.set_gui(&Arc::from(list), &[], &Default::default());
        r.render_to_image(48, 16, &camera).unwrap()
    };
    draw(&mut kept, &[a.clone(), b, c.clone()]);
    for list in [
        // The same size: written alone.
        vec![a.clone(), piece(vec![square(24.0, blue)]), c.clone()],
        // Grown: the pieces after it move along.
        vec![
            a.clone(),
            piece(vec![square(24.0, red), square(24.0, green)]),
            c.clone(),
        ],
        // One fewer.
        vec![a, c],
    ] {
        assert_eq!(draw(&mut kept, &list), draw(&mut fresh, &list));
    }
}
