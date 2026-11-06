//! Scene queries. Expected values were measured in Studio 0.742.

use super::*;
use std::f32::consts::FRAC_1_SQRT_2;

use crate::query::{CastShape, Filter, MAX_RAY_LENGTH, Region};

const O: Vec3 = Vec3::new(0.0, 1000.0, 0.0);

/// An anchored 2-stud cube at `at` from [`O`].
fn fixed(dom: &mut Dom, at: Vec3) -> InstanceId {
    block(dom, O + at, true)
}

fn all() -> Filter {
    Filter::legacy_ray(Vec::new(), false)
}

fn ray(physics: &mut Physics, dom: &mut Dom, from: Vec3, dir: Vec3) -> Option<query::Hit> {
    physics.raycast(dom, O + from, dir, &all())
}

fn region(physics: &mut Physics, dom: &mut Dom, min: Vec3, max: Vec3) -> Vec<InstanceId> {
    let region = Region::Box {
        cframe: CFrame::from_position(O + (min + max) * 0.5),
        size: max - min,
    };
    physics.parts_in_bounds(dom, region, &Filter::legacy_region(vec![], false), 0)
}

#[test]
fn rays_hit_the_nearest_face() {
    let mut dom = Dom::new();
    let near = fixed(&mut dom, Vec3::new(0.0, 0.0, -10.0));
    fixed(&mut dom, Vec3::new(0.0, 0.0, -20.0));
    let mut physics = Physics::new();
    let hit = ray(
        &mut physics,
        &mut dom,
        Vec3::ZERO,
        Vec3::new(0.0, 0.0, -100.0),
    )
    .unwrap();
    assert_eq!(hit.part, near);
    assert!((hit.distance - 9.0).abs() < 1e-3, "{hit:?}");
    assert!((hit.position - (O + Vec3::new(0.0, 0.0, -9.0))).length() < 1e-3);
    assert!((hit.normal - Vec3::Z).length() < 1e-4);
    // Too short to reach.
    assert!(
        ray(
            &mut physics,
            &mut dom,
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, -5.0)
        )
        .is_none()
    );
    // Ignored.
    let far = physics
        .raycast(
            &mut dom,
            O,
            Vec3::new(0.0, 0.0, -100.0),
            &Filter::legacy_ray(vec![near], false),
        )
        .unwrap();
    assert!((far.distance - 19.0).abs() < 1e-3);
}

#[test]
fn rays_skip_parts_they_start_in() {
    let mut dom = Dom::new();
    part(&mut dom, O, Vec3::splat(4.0), true);
    let behind = part(
        &mut dom,
        O + Vec3::new(0.0, 0.0, -3.0),
        Vec3::splat(4.0),
        true,
    );
    let mut physics = Physics::new();
    // Observed: it hits the overlapping part behind at its face, z = -1.
    let hit = ray(
        &mut physics,
        &mut dom,
        Vec3::ZERO,
        Vec3::new(0.0, 0.0, -100.0),
    )
    .unwrap();
    assert_eq!(hit.part, behind);
    assert!((hit.position.z + 1.0).abs() < 1e-3);
}

#[test]
fn can_query_only_matters_without_can_collide() {
    let mut dom = Dom::new();
    let a = fixed(&mut dom, Vec3::new(0.0, 0.0, -10.0));
    let mut physics = Physics::new();
    let hits = |physics: &mut Physics, dom: &mut Dom| {
        ray(physics, dom, Vec3::ZERO, Vec3::new(0.0, 0.0, -100.0)).is_some()
    };
    set(&mut dom, a, "CanQuery", Value::Bool(false));
    assert!(hits(&mut physics, &mut dom));
    set(&mut dom, a, "CanCollide", Value::Bool(false));
    assert!(!hits(&mut physics, &mut dom));
    set(&mut dom, a, "CanQuery", Value::Bool(true));
    assert!(hits(&mut physics, &mut dom));
    let respect = Filter {
        respect_can_collide: true,
        ..Filter::default()
    };
    assert!(
        physics
            .raycast(&mut dom, O, Vec3::new(0.0, 0.0, -100.0), &respect)
            .is_none()
    );
}

#[test]
fn rays_reach_15000_studs() {
    let mut dom = Dom::new();
    let p = fixed(&mut dom, Vec3::new(0.0, 0.0, -14_999.0));
    let mut physics = Physics::new();
    let long = Vec3::new(0.0, 0.0, -100_000.0);
    assert!(ray(&mut physics, &mut dom, Vec3::ZERO, long).is_some());
    set(
        &mut dom,
        p,
        "CFrame",
        Value::CFrame(CFrame::from_position(
            O + Vec3::new(0.0, 0.0, -MAX_RAY_LENGTH - 2.0),
        )),
    );
    assert!(ray(&mut physics, &mut dom, Vec3::ZERO, long).is_none());
}

#[test]
fn queries_see_this_frames_changes() {
    let mut dom = Dom::new();
    let floor = part(
        &mut dom,
        O + Vec3::new(0.0, -5.0, 0.0),
        Vec3::new(20.0, 1.0, 20.0),
        true,
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.1);
    // Created after the last step.
    let new = fixed(&mut dom, Vec3::new(0.0, 0.0, -10.0));
    let down = Vec3::new(0.0, 0.0, -100.0);
    assert_eq!(
        ray(&mut physics, &mut dom, Vec3::ZERO, down).map(|h| h.part),
        Some(new)
    );
    // Moved after it.
    set(
        &mut dom,
        new,
        "CFrame",
        Value::CFrame(CFrame::from_position(O + Vec3::new(50.0, 0.0, -10.0))),
    );
    assert!(ray(&mut physics, &mut dom, Vec3::ZERO, down).is_none());
    let hit = ray(&mut physics, &mut dom, Vec3::new(50.0, 0.0, 0.0), down).unwrap();
    assert_eq!(hit.part, new);
    // Destroyed.
    dom.destroy(new).unwrap();
    assert!(ray(&mut physics, &mut dom, Vec3::new(50.0, 0.0, 0.0), down).is_none());
    // A falling part is found where physics put it.
    let ball = part(
        &mut dom,
        O + Vec3::new(5.0, 0.0, 0.0),
        Vec3::splat(1.0),
        false,
    );
    run(&mut physics, &mut dom, 1.0);
    let y = position(&dom, ball).y;
    assert!((y - (O.y - 4.0)).abs() < 0.1, "{y}");
    let hit = ray(
        &mut physics,
        &mut dom,
        Vec3::new(5.0, 10.0, 0.0),
        Vec3::new(0.0, -100.0, 0.0),
    )
    .unwrap();
    assert_eq!(hit.part, ball);
    let _ = floor;
}

/// A box with a NaN or infinite number in it, from wherever it came
/// (scripts can't write one), is no part of the world, and the rest still
/// is: such a box made the bounds tree panic.
#[test]
fn parts_with_nan_boxes_are_left_out() {
    let mut dom = Dom::new();
    let floor = part(
        &mut dom,
        O + Vec3::new(0.0, -5.0, 0.0),
        Vec3::new(20.0, 1.0, 20.0),
        true,
    );
    let loose = part(&mut dom, O, Vec3::splat(2.0), false);
    let still = fixed(&mut dom, Vec3::new(5.0, 0.0, 0.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.1);
    let near = Vec3::splat(30.0);
    assert!(region(&mut physics, &mut dom, -near, near).contains(&still));
    let nan = Vec3::new(f32::NAN, 0.0, 0.0);
    set(
        &mut dom,
        still,
        "CFrame",
        Value::CFrame(CFrame::from_position(nan)),
    );
    let wide = Vec3::new(f32::INFINITY, 1.0, 1.0);
    set(
        &mut dom,
        loose,
        "Size",
        Value::Vector3(rook_types::Vector3(wide)),
    );
    run(&mut physics, &mut dom, 0.1);
    let found = region(&mut physics, &mut dom, -near, near);
    assert!(
        found.contains(&floor) && !found.contains(&still),
        "{found:?}"
    );
    let down = Vec3::new(0.0, -100.0, 0.0);
    let hit = ray(&mut physics, &mut dom, Vec3::new(8.0, 10.0, 0.0), down);
    assert_eq!(hit.map(|h| h.part), Some(floor));
}

#[test]
fn ignore_lists_cover_descendants() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let model = child(&mut dom, "Model", ws, &[]);
    let inside = fixed(&mut dom, Vec3::new(0.0, 0.0, -10.0));
    dom.set_parent(inside, Some(model)).unwrap();
    let behind = fixed(&mut dom, Vec3::new(0.0, 0.0, -20.0));
    let mut physics = Physics::new();
    let dir = Vec3::new(0.0, 0.0, -100.0);
    let hit = |physics: &mut Physics, dom: &mut Dom, include| {
        physics
            .raycast(dom, O, dir, &Filter::legacy_ray(vec![model], include))
            .map(|h| h.part)
    };
    assert_eq!(hit(&mut physics, &mut dom, false), Some(behind));
    assert_eq!(hit(&mut physics, &mut dom, true), Some(inside));
}

#[test]
fn legacy_rays_act_as_the_default_group() {
    let mut dom = Dom::new();
    let p = fixed(&mut dom, Vec3::new(0.0, 0.0, -10.0));
    let mut groups = groups::CollisionGroups::read(&dom);
    groups.register("X").unwrap();
    groups.set_collidable("X", "Default", false).unwrap();
    groups.write(&mut dom).unwrap();
    set(&mut dom, p, "CollisionGroup", Value::String("X".into()));
    let mut physics = Physics::new();
    let dir = Vec3::new(0.0, 0.0, -100.0);
    assert!(ray(&mut physics, &mut dom, Vec3::ZERO, dir).is_none());
    let as_x = Filter {
        group: Some("X".into()),
        ..Filter::default()
    };
    assert!(physics.raycast(&mut dom, O, dir, &as_x).is_some());
    // Regions ignore groups.
    let found = region(
        &mut physics,
        &mut dom,
        Vec3::splat(-20.0),
        Vec3::splat(20.0),
    );
    assert_eq!(found, vec![p]);
}

/// A corner wedge is tall above its +X -Z corner and flat at -X +Z.
#[test]
fn corner_wedge_apex_is_at_plus_x_minus_z() {
    let mut dom = Dom::new();
    part_of(&mut dom, "CornerWedgePart", O, Vec3::splat(2.0), true);
    let mut physics = Physics::new();
    let mut down = |x: f32, z: f32| {
        let from = Vec3::new(x, 10.0, z);
        let hit = ray(&mut physics, &mut dom, from, Vec3::new(0.0, -20.0, 0.0));
        hit.unwrap().position.y - O.y
    };
    assert!(down(0.9, -0.9) > 0.7);
    assert!(down(-0.9, 0.9) < -0.7);
}

#[test]
fn rays_hit_balls_cylinders_and_wedges_by_their_shape() {
    let mut dom = Dom::new();
    let ball = part(
        &mut dom,
        O + Vec3::new(0.0, 0.0, -10.0),
        Vec3::splat(4.0),
        true,
    );
    set(&mut dom, ball, "Shape", Value::Enum(0));
    let wedge = part_of(
        &mut dom,
        "WedgePart",
        O + Vec3::new(80.0, 0.0, 0.0),
        Vec3::splat(2.0),
        true,
    );
    let mut physics = Physics::new();
    let dir = Vec3::new(0.0, 0.0, -100.0);
    // Grazing the box's corner misses the sphere.
    assert!(ray(&mut physics, &mut dom, Vec3::new(1.9, 1.9, 0.0), dir).is_none());
    // Observed: from x = 1 it hits at z = -8.27 with normal (0.5, 0, 0.87).
    let hit = ray(&mut physics, &mut dom, Vec3::new(1.0, 0.0, 0.0), dir).unwrap();
    assert!((hit.position.z + 8.268).abs() < 1e-2, "{hit:?}");
    assert!((hit.normal - Vec3::new(0.5, 0.0, 0.866)).length() < 1e-2);
    // Observed: the slope's normal is (0, 0.71, -0.71), at y = 0.9 near the back.
    let from = Vec3::new(80.0, 10.0, 0.9);
    let hit = ray(&mut physics, &mut dom, from, Vec3::new(0.0, -20.0, 0.0)).unwrap();
    assert_eq!(hit.part, wedge);
    assert!((hit.position.y - O.y - 0.9).abs() < 1e-3);
    assert!((hit.normal - Vec3::new(0.0, FRAC_1_SQRT_2, -FRAC_1_SQRT_2)).length() < 1e-3);
}

#[test]
fn regions_test_size_boxes_and_count_touching_faces() {
    let mut dom = Dom::new();
    let c = fixed(&mut dom, Vec3::ZERO);
    let b = fixed(&mut dom, Vec3::new(4.0, 0.0, 0.0));
    let ball = part(
        &mut dom,
        O + Vec3::new(50.0, 0.0, 0.0),
        Vec3::splat(4.0),
        true,
    );
    set(&mut dom, ball, "Shape", Value::Enum(0));
    let rot = fixed(&mut dom, Vec3::new(100.0, 0.0, 0.0));
    set(
        &mut dom,
        rot,
        "Size",
        Value::Vector3(Vector3(Vec3::splat(4.0))),
    );
    let turned = CFrame {
        position: O + Vec3::new(100.0, 0.0, 0.0),
        rotation: glam::Mat3::from_rotation_y(std::f32::consts::FRAC_PI_4),
    };
    set(&mut dom, rot, "CFrame", Value::CFrame(turned));
    let mut physics = Physics::new();
    let mut r = |a: [f32; 3], b: [f32; 3]| region(&mut physics, &mut dom, a.into(), b.into());
    assert_eq!(r([-10.0; 3], [10.0; 3]), vec![c, b]);
    assert_eq!(r([1.0, -1.0, -1.0], [2.0, 1.0, 1.0]), vec![c]);
    assert_eq!(r([5.0, -1.0, -1.0], [6.0, 1.0, 1.0]), vec![b]);
    assert_eq!(r([5.01, -1.0, -1.0], [6.0, 1.0, 1.0]), vec![]);
    assert_eq!(r([51.7, 1.7, 1.7], [52.0, 2.0, 2.0]), vec![ball]);
    assert_eq!(r([102.5, -1.0, 2.5], [102.8, 1.0, 2.8]), vec![]);
}

#[test]
fn region_results_stop_at_max_parts() {
    let mut dom = Dom::new();
    let made: Vec<_> = (1..=40)
        .map(|i| fixed(&mut dom, Vec3::new(0.0, i as f32 * 3.0, 0.0)))
        .collect();
    let mut physics = Physics::new();
    let region = Region::Box {
        cframe: CFrame::from_position(O + Vec3::new(0.0, 60.0, 0.0)),
        size: Vec3::new(20.0, 200.0, 20.0),
    };
    let filter = Filter::legacy_region(vec![], false);
    let found = physics.parts_in_bounds(&mut dom, region, &filter, 20);
    assert_eq!(found, made[..20]);
    let found = physics.parts_in_bounds(&mut dom, region, &filter, 0);
    assert_eq!(found.len(), 40);
}

#[test]
fn parts_in_part_use_real_shapes_and_skip_the_part() {
    let mut dom = Dom::new();
    let ball = part(
        &mut dom,
        O + Vec3::new(50.0, 0.0, 0.0),
        Vec3::splat(4.0),
        true,
    );
    set(&mut dom, ball, "Shape", Value::Enum(0));
    let probe = part(
        &mut dom,
        O + Vec3::new(51.85, 1.85, 1.85),
        Vec3::splat(0.2),
        true,
    );
    let inner = part(
        &mut dom,
        O + Vec3::new(51.0, 0.0, 0.0),
        Vec3::splat(0.2),
        true,
    );
    let mut physics = Physics::new();
    let f = Filter::default();
    assert!(physics.parts_in_part(&mut dom, probe, &f, 0).is_empty());
    assert_eq!(physics.parts_in_part(&mut dom, inner, &f, 0), vec![ball]);
    assert_eq!(physics.parts_in_part(&mut dom, ball, &f, 0), vec![inner]);
}

#[test]
fn touching_parts_need_overlap() {
    let mut dom = Dom::new();
    let a = fixed(&mut dom, Vec3::ZERO);
    let over = fixed(&mut dom, Vec3::new(1.0, 0.0, 0.0));
    let face = fixed(&mut dom, Vec3::new(0.0, 2.0, 0.0));
    let ghost = fixed(&mut dom, Vec3::new(0.0, 0.0, 1.0));
    set(&mut dom, ghost, "CanCollide", Value::Bool(false));
    let shy = fixed(&mut dom, Vec3::new(0.0, 0.0, -1.0));
    set(&mut dom, shy, "CanTouch", Value::Bool(false));
    let mut physics = Physics::new();
    assert_eq!(physics.touching_parts(&mut dom, a, false), vec![over, shy]);
    // A non-colliding part reports nothing, unless its Touched is
    // connected; then everything it overlaps.
    assert!(physics.touching_parts(&mut dom, ghost, false).is_empty());
    let mut all = physics.touching_parts(&mut dom, ghost, true);
    all.sort();
    assert_eq!(all, vec![a, over]);
    let _ = face;
}

#[test]
fn sphere_casts_report_travel_and_skip_parts_they_start_in() {
    let mut dom = Dom::new();
    let wall = part(
        &mut dom,
        O + Vec3::new(0.0, 0.0, -10.0),
        Vec3::new(10.0, 10.0, 1.0),
        true,
    );
    let mut physics = Physics::new();
    let cast = |physics: &mut Physics, dom: &mut Dom, shape, z: f32| {
        let cf = CFrame::from_position(O + Vec3::new(0.0, 0.0, z));
        physics.shapecast(
            dom,
            shape,
            cf,
            Vec3::new(0.0, 0.0, -100.0),
            &Filter::default(),
        )
    };
    let hit = cast(&mut physics, &mut dom, CastShape::Sphere(1.0), 0.0).unwrap();
    assert_eq!(hit.part, wall);
    assert!((hit.distance - 8.5).abs() < 1e-2, "{hit:?}");
    assert!((hit.position.z + 9.5).abs() < 1e-2);
    assert!((hit.normal - Vec3::Z).length() < 1e-2);
    let hit = cast(
        &mut physics,
        &mut dom,
        CastShape::Block(Vec3::splat(2.0)),
        0.0,
    )
    .unwrap();
    assert!((hit.distance - 8.5).abs() < 1e-2, "{hit:?}");
    assert!(cast(&mut physics, &mut dom, CastShape::Sphere(1.0), -9.0).is_none());
}

#[test]
fn worlds_owning_nothing_never_simulate() {
    let mut dom = Dom::new();
    let p = block(&mut dom, O, false);
    let mut physics = Physics::with_authority(Authority::Only(Vec::new()));
    run(&mut physics, &mut dom, 0.5);
    assert_eq!(position(&dom, p), O);
    let hit = ray(
        &mut physics,
        &mut dom,
        Vec3::new(0.0, 10.0, 0.0),
        Vec3::NEG_Y * 20.0,
    );
    assert_eq!(hit.map(|h| h.part), Some(p));
    // Moves arrive from replication.
    let moved = O + Vec3::new(30.0, 0.0, 0.0);
    set(
        &mut dom,
        p,
        "CFrame",
        Value::CFrame(CFrame::from_position(moved)),
    );
    let hit = ray(
        &mut physics,
        &mut dom,
        Vec3::new(30.0, 10.0, 0.0),
        Vec3::NEG_Y * 20.0,
    );
    assert_eq!(hit.map(|h| h.part), Some(p));
}

#[test]
fn click_rays_pass_invisible_parts_that_dont_collide() {
    let mut dom = Dom::new();
    let behind = fixed(&mut dom, Vec3::new(0.0, 0.0, -20.0));
    let ghost = fixed(&mut dom, Vec3::new(0.0, 0.0, -10.0));
    set(&mut dom, ghost, "Transparency", Value::Float32(1.0));
    set(&mut dom, ghost, "CanCollide", Value::Bool(false));
    let mut physics = Physics::new();
    let dir = Vec3::new(0.0, 0.0, -100.0);
    let clicks = Filter {
        skip_invisible_ghosts: true,
        ..all()
    };
    let hit = |physics: &mut Physics, dom: &mut Dom, f: &Filter| {
        physics.raycast(dom, O, dir, f).map(|h| h.part)
    };
    // Observed: Mouse.Target still reports the invisible part.
    assert_eq!(hit(&mut physics, &mut dom, &all()), Some(ghost));
    assert_eq!(hit(&mut physics, &mut dom, &clicks), Some(behind));
    // Colliding, or visible, it blocks clicks.
    set(&mut dom, ghost, "CanCollide", Value::Bool(true));
    assert_eq!(hit(&mut physics, &mut dom, &clicks), Some(ghost));
    set(&mut dom, ghost, "CanCollide", Value::Bool(false));
    set(&mut dom, ghost, "Transparency", Value::Float32(0.0));
    assert_eq!(hit(&mut physics, &mut dom, &clicks), Some(ghost));
}

/// The parts' box index finds exactly the parts whose `Size` box meets a
/// query box, however they got there: placed, fallen by stepping, moved
/// and resized by scripts, removed, and a turned cylinder near its box's
/// corner, which its round collider's bounds don't reach.
#[test]
fn box_queries_find_parts_by_their_box_through_every_change() {
    let mut dom = Dom::new();
    let floor = part(&mut dom, O, Vec3::new(40.0, 1.0, 40.0), true);
    let falling = block(&mut dom, O + Vec3::new(5.0, 10.0, 0.0), false);
    let moved = fixed(&mut dom, Vec3::new(-8.0, 3.0, 0.0));
    let grown = fixed(&mut dom, Vec3::new(0.0, 3.0, 8.0));
    let gone = fixed(&mut dom, Vec3::new(0.0, 3.0, -8.0));
    let round = part(
        &mut dom,
        O + Vec3::new(10.0, 4.0, 10.0),
        Vec3::splat(4.0),
        true,
    );
    set(&mut dom, round, "Shape", Value::Enum(2));
    let turned = CFrame::from_position(O + Vec3::new(10.0, 4.0, 10.0))
        * CFrame::angles(std::f32::consts::FRAC_PI_4, 0.0, 0.0);
    set(&mut dom, round, "CFrame", Value::CFrame(turned));
    let mut physics = Physics::new();
    let check = |physics: &mut Physics, dom: &mut Dom, when: &str| {
        let parts = crate::shape::workspace_parts(dom);
        for x in (-12..=12).step_by(3) {
            for y in -1..=12 {
                for z in (-12..=12).step_by(3) {
                    let lo = O + Vec3::new(x as f32, y as f32, z as f32) - 0.6;
                    let hi = lo + 1.2;
                    let mut truth: Vec<InstanceId> = parts
                        .iter()
                        .copied()
                        .filter(|&p| {
                            let cf = dom.cframe_property(p, "CFrame").unwrap();
                            let reach = cf.box_reach(dom.vector3_property(p, "Size").unwrap());
                            let (a, b) = (cf.position - reach, cf.position + reach);
                            a.cmple(hi).all() && b.cmpge(lo).all()
                        })
                        .collect();
                    truth.sort_unstable();
                    physics.sync(dom);
                    assert_eq!(
                        physics.parts_by_box(dom, lo, hi),
                        truth,
                        "{when} at {x} {y} {z}"
                    );
                }
            }
        }
    };
    check(&mut physics, &mut dom, "placed");
    // Near the turned cylinder's box corner, outside its round shape.
    let corner = O + Vec3::new(10.0, 4.0 + 2.6, 10.0);
    physics.sync(&mut dom);
    let near_corner = physics.parts_by_box(&dom, corner - 0.1, corner + 0.1);
    assert_eq!(near_corner, vec![round]);
    let region = Region::Box {
        cframe: CFrame::from_position(corner),
        size: Vec3::splat(0.2),
    };
    let filter = Filter::legacy_region(vec![], false);
    assert_eq!(
        physics.parts_in_bounds(&mut dom, region, &filter, 0),
        vec![round]
    );
    run(&mut physics, &mut dom, 1.0);
    assert!(position(&dom, falling).y < O.y + 5.0, "it fell");
    check(&mut physics, &mut dom, "fallen");
    set(
        &mut dom,
        moved,
        "CFrame",
        Value::CFrame(CFrame::from_position(O + Vec3::new(-2.0, 8.0, 2.0))),
    );
    set(
        &mut dom,
        grown,
        "Size",
        Value::Vector3(Vector3(Vec3::new(10.0, 2.0, 2.0))),
    );
    dom.set_parent(gone, None).unwrap();
    check(&mut physics, &mut dom, "moved, grown and removed");
    run(&mut physics, &mut dom, 0.5);
    check(&mut physics, &mut dom, "stepped again");
    // Enough parts that one more goes in on its own, not by a bulk build.
    for i in 0..40 {
        fixed(&mut dom, Vec3::new(30.0 + 3.0 * i as f32, 3.0, 30.0));
    }
    check(&mut physics, &mut dom, "many added");
    fixed(&mut dom, Vec3::new(2.0, 3.0, -2.0));
    check(&mut physics, &mut dom, "one added");
    // A nudge inside the leaf's skin: the tree keeps the leaf, and the
    // part is found by where its box is now, not where it was.
    let cf = dom.cframe_property(moved, "CFrame").unwrap();
    let left = cf.position.x - dom.vector3_property(moved, "Size").unwrap().x / 2.0;
    set(
        &mut dom,
        moved,
        "CFrame",
        Value::CFrame(CFrame::from_position(cf.position + Vec3::X * 0.02)),
    );
    let at = |x: f32| Vec3::new(x, cf.position.y, cf.position.z);
    physics.sync(&mut dom);
    let old_edge = physics.parts_by_box(&dom, at(left - 0.1), at(left + 0.01));
    assert!(!old_edge.contains(&moved), "found where it was");
    let new_edge = physics.parts_by_box(&dom, at(left + 0.015), at(left + 0.025));
    assert!(new_edge.contains(&moved), "missed where it is");
    let _ = floor;
}

/// A box query can't read the part boxes behind the DOM: physics must
/// have taken its changes first.
#[test]
#[should_panic(expected = "physics hasn't taken the DOM's latest changes")]
fn box_queries_refuse_an_unsynced_dom() {
    let mut dom = Dom::new();
    let p = fixed(&mut dom, Vec3::ZERO);
    let mut physics = Physics::new();
    physics.sync(&mut dom);
    let moved = CFrame::from_position(O + Vec3::new(5.0, 0.0, 0.0));
    set(&mut dom, p, "CFrame", Value::CFrame(moved));
    physics.parts_by_box(&dom, O - 1.0, O + 1.0);
}

/// A wedge wholly inside a block rises onto it (Studio 0.746), though no
/// contact comes back for a hull inside a shape.
#[test]
fn a_wedge_rises_out_of_a_block() {
    let mut dom = Dom::new();
    part(&mut dom, Vec3::ZERO, Vec3::new(4.0, 2.0, 4.0), true);
    let wedge = part_of(
        &mut dom,
        "WedgePart",
        Vec3::new(0.0, 50.0, 0.0),
        Vec3::splat(2.0),
        true,
    );
    let mut physics = Physics::new();
    let rise = physics.rise_out_of_parts(&mut dom, &[(wedge, CFrame::IDENTITY)]);
    assert!((rise - 2.0).abs() < 1e-3, "{rise}");
}
