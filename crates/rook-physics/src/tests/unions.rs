//! Unions collide and are queried as their geometry, not their box.

use super::*;
use std::sync::Arc;

use rook_types::{Hull, UnionCollision};

use crate::query::Filter;

const O: Vec3 = Vec3::new(0.0, 1000.0, 0.0);

/// A box's corners, as a hull.
fn cuboid(min: Vec3, max: Vec3) -> Hull {
    let positions = (0..8)
        .map(|i| {
            let pick = |bit, lo: f32, hi: f32| if i & bit == 0 { lo } else { hi };
            [
                pick(1, min.x, max.x),
                pick(2, min.y, max.y),
                pick(4, min.z, max.z),
            ]
        })
        .collect();
    Hull {
        positions,
        indices: Vec::new(),
    }
}

/// Two 2-stud blocks with a 2-stud gap between them along X: a 6 × 2 × 2
/// union.
fn gap() -> Arc<UnionCollision> {
    Arc::new(UnionCollision::Hulls(vec![
        cuboid(Vec3::new(-3.0, -1.0, -1.0), Vec3::new(-1.0, 1.0, 1.0)),
        cuboid(Vec3::new(1.0, -1.0, -1.0), Vec3::new(3.0, 1.0, 1.0)),
    ]))
}

fn union(dom: &mut Dom, at: Vec3, anchored: bool) -> InstanceId {
    let id = part(
        dom,
        "UnionOperation",
        O + at,
        Vec3::new(6.0, 2.0, 2.0),
        anchored,
    );
    set(dom, id, "InitialSize", v3(6.0, 2.0, 2.0));
    id
}

/// What a ray down -Z at `x` hits first.
fn ray_at(physics: &mut Physics, dom: &mut Dom, x: f32) -> Option<f32> {
    let filter = Filter::legacy_ray(Vec::new(), false);
    physics
        .raycast(
            dom,
            O + Vec3::new(x, 0.0, 5.0),
            Vec3::new(0.0, 0.0, -20.0),
            &filter,
        )
        .map(|h| (h.distance * 1000.0).round() / 1000.0)
}

#[test]
fn rays_pass_through_a_unions_gaps() {
    let mut dom = Dom::new();
    let u = union(&mut dom, Vec3::ZERO, true);
    let mut physics = Physics::new();
    // Its box until the geometry arrives.
    assert_eq!(ray_at(&mut physics, &mut dom, 0.0), Some(4.0));
    physics.set_union_collision(u, Some(gap()));
    assert_eq!(ray_at(&mut physics, &mut dom, 0.0), None);
    assert_eq!(ray_at(&mut physics, &mut dom, 2.0), Some(4.0));
    // A block collides as its box.
    physics.set_union_collision(u, Some(Arc::new(UnionCollision::Block)));
    assert_eq!(ray_at(&mut physics, &mut dom, 0.0), Some(4.0));
}

#[test]
fn union_geometry_scales_with_size() {
    let mut dom = Dom::new();
    let u = union(&mut dom, Vec3::ZERO, true);
    let mut physics = Physics::new();
    physics.set_union_collision(u, Some(gap()));
    set(&mut dom, u, "Size", v3(12.0, 2.0, 4.0));
    // The gap is now x in (-2, 2), and the faces 2 studs from the centre.
    assert_eq!(ray_at(&mut physics, &mut dom, 1.5), None);
    assert_eq!(ray_at(&mut physics, &mut dom, 3.0), Some(3.0));
}

#[test]
fn parts_fall_through_a_unions_gaps() {
    let mut dom = Dom::new();
    let flat = union(&mut dom, Vec3::ZERO, true);
    let boxed = union(&mut dom, Vec3::new(20.0, 0.0, 0.0), true);
    let mut physics = Physics::new();
    physics.set_union_collision(flat, Some(gap()));
    let through = part(
        &mut dom,
        "Part",
        O + Vec3::new(0.0, 3.0, 0.0),
        Vec3::ONE,
        false,
    );
    let on = part(
        &mut dom,
        "Part",
        O + Vec3::new(20.0, 3.0, 0.0),
        Vec3::ONE,
        false,
    );
    run(&mut physics, &mut dom, 1.0);
    assert!(
        position(&dom, through).y < O.y - 5.0,
        "{:?}",
        position(&dom, through)
    );
    assert!((position(&dom, on).y - (O.y + 1.5)).abs() < 0.05);
    let _ = boxed;
}

#[test]
fn a_union_without_saved_hulls_collides_as_its_mesh() {
    let mut dom = Dom::new();
    let u = union(&mut dom, Vec3::ZERO, true);
    let mut physics = Physics::new();
    // A closed 2-stud cube in the middle of the 6-stud box.
    let cube = cuboid(Vec3::splat(-1.0), Vec3::splat(1.0));
    let indices = vec![
        0, 2, 1, 1, 2, 3, 4, 5, 6, 5, 7, 6, 0, 1, 4, 1, 5, 4, 2, 6, 3, 3, 6, 7, 0, 4, 2, 2, 4, 6,
        1, 3, 5, 3, 7, 5,
    ];
    let mesh = Hull {
        positions: cube.positions,
        indices,
    };
    physics.set_union_collision(u, Some(Arc::new(UnionCollision::Mesh(mesh))));
    assert_eq!(ray_at(&mut physics, &mut dom, 2.0), None);
    assert_eq!(ray_at(&mut physics, &mut dom, 0.0), Some(4.0));
}

/// A moving union that doesn't collide keeps its box (for speed).
#[test]
fn moving_non_colliding_unions_keep_their_box() {
    let mut dom = Dom::new();
    let u = union(&mut dom, Vec3::ZERO, false);
    set(&mut dom, u, "CanCollide", Value::Bool(false));
    let mut physics = Physics::new();
    physics.set_union_collision(u, Some(gap()));
    assert_eq!(ray_at(&mut physics, &mut dom, 0.0), Some(4.0));
}
