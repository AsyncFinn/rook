use super::*;

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
