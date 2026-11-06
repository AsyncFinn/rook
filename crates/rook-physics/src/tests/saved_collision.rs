//! Unions and MeshParts collide, are queried and weigh as the geometry saved
//! in them, not their box.

use super::*;

use rook_content::decode::mesh::union::physics;
use rook_types::Hull;

use crate::query::Filter;

const O: Vec3 = Vec3::new(0.0, 1000.0, 0.0);

/// A box's corners and faces, as a hull.
fn cuboid(min: Vec3, max: Vec3) -> Hull {
    let positions = (0..8)
        .map(|i| {
            let pick = |bit, lo: f32, hi: f32| if i & bit == 0 { lo } else { hi };
            [
                pick(4, min.x, max.x),
                pick(2, min.y, max.y),
                pick(1, min.z, max.z),
            ]
        })
        .collect();
    let indices = vec![
        0, 1, 3, 0, 3, 2, 4, 6, 7, 4, 7, 5, 0, 4, 5, 0, 5, 1, 2, 3, 7, 2, 7, 6, 0, 2, 6, 0, 6, 4,
        1, 5, 7, 1, 7, 3,
    ];
    Hull { positions, indices }
}

/// Two 2-stud blocks with a 2-stud gap between them along X: a 6 × 2 × 2
/// shape.
fn gap() -> Value {
    Value::BinaryString(
        physics::encode(&[
            cuboid(Vec3::new(-3.0, -1.0, -1.0), Vec3::new(-1.0, 1.0, 1.0)),
            cuboid(Vec3::new(1.0, -1.0, -1.0), Vec3::new(3.0, 1.0, 1.0)),
        ])
        .into(),
    )
}

fn saved(dom: &mut Dom, id: InstanceId, blob: Value) {
    set(dom, id, "PhysicalConfigData", blob);
}

fn shaped(dom: &mut Dom, class: &str, at: Vec3, anchored: bool) -> InstanceId {
    let id = part_of(dom, class, O + at, Vec3::new(6.0, 2.0, 2.0), anchored);
    set(dom, id, "InitialSize", v3(6.0, 2.0, 2.0));
    id
}

fn union(dom: &mut Dom, at: Vec3, anchored: bool) -> InstanceId {
    shaped(dom, "UnionOperation", at, anchored)
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
fn rays_pass_through_saved_gaps() {
    for class in ["UnionOperation", "MeshPart"] {
        let mut dom = Dom::new();
        let u = shaped(&mut dom, class, Vec3::ZERO, true);
        let mut physics = Physics::new();
        // Its box without geometry.
        assert_eq!(ray_at(&mut physics, &mut dom, 0.0), Some(4.0));
        saved(&mut dom, u, gap());
        assert_eq!(ray_at(&mut physics, &mut dom, 0.0), None, "{class}");
        assert_eq!(ray_at(&mut physics, &mut dom, 2.0), Some(4.0));
        // A block collides as its box.
        saved(
            &mut dom,
            u,
            Value::BinaryString(b"CSGPHS\0\0\0\0BLOCK".to_vec().into()),
        );
        assert_eq!(ray_at(&mut physics, &mut dom, 0.0), Some(4.0));
    }
}

#[test]
fn older_files_save_it_in_physics_data() {
    let mut dom = Dom::new();
    let u = shaped(&mut dom, "MeshPart", Vec3::ZERO, true);
    set(&mut dom, u, "PhysicsData", gap());
    let mut physics = Physics::new();
    assert_eq!(ray_at(&mut physics, &mut dom, 0.0), None);
}

#[test]
fn saved_geometry_scales_with_size() {
    let mut dom = Dom::new();
    let u = union(&mut dom, Vec3::ZERO, true);
    let mut physics = Physics::new();
    saved(&mut dom, u, gap());
    set(&mut dom, u, "Size", v3(12.0, 2.0, 4.0));
    // The gap is now x in (-2, 2), and the faces 2 studs from the center.
    assert_eq!(ray_at(&mut physics, &mut dom, 1.5), None);
    assert_eq!(ray_at(&mut physics, &mut dom, 3.0), Some(3.0));
}

/// Saved without `InitialSize`, hulls stretch from their own bounds to
/// `Size`, and the part weighs as its box (Studio 0.746).
#[test]
fn no_initial_size_stretches_from_the_hulls_bounds() {
    let mut dom = Dom::new();
    let u = part_of(&mut dom, "MeshPart", O, Vec3::new(12.0, 2.0, 2.0), true);
    saved(&mut dom, u, gap());
    let mut physics = Physics::new();
    assert_eq!(ray_at(&mut physics, &mut dom, 1.5), None);
    assert_eq!(ray_at(&mut physics, &mut dom, 3.0), Some(4.0));
    assert!((physics.part_mass(&dom, u) - 12.0 * 2.0 * 2.0 * 0.7).abs() < 1e-3);
}

/// Numbers from Studio: `GetMass` and `AssemblyCenterOfMass` follow the
/// saved mass, and the part turns on it.
#[test]
fn saved_mass_weighs_and_turns_the_part() {
    let mut dom = Dom::new();
    let u = shaped(&mut dom, "MeshPart", Vec3::ZERO, false);
    set(&mut dom, u, "Size", v3(12.0, 2.0, 2.0));
    let mut blob = b"CSGPHS\x06\0\0\0".to_vec();
    for f in [1.5_f32, -0.25, 0.5, 0.0, 3.0, 0.0, 0.0, 5.0, 0.0, 7.0] {
        blob.extend_from_slice(&f.to_le_bytes());
    }
    let Value::BinaryString(hulls) = gap() else {
        unreachable!()
    };
    blob.extend_from_slice(&AsRef::<[u8]>::as_ref(&hulls)[10..]);
    saved(&mut dom, u, Value::BinaryString(blob.into()));
    let mut physics = Physics::new();
    physics.sync(&mut dom);
    // Twice as long, so 3 at Plastic's 0.7.
    assert!((physics.part_mass(&dom, u) - 2.1).abs() < 1e-5);
    let asm = Assembly::of(&dom, u).unwrap();
    assert!((asm.mass(&dom, &physics) - 2.1).abs() < 1e-5);
    let center = asm.center_of_mass(&dom, &physics) - O;
    assert!(
        (center - Vec3::new(-0.5, 0.5, 0.0)).length() < 1e-4,
        "{center}"
    );
    let body = physics.body_of(u).unwrap();
    let props = physics.world.bodies[body].mass_properties().local_mprops;
    assert!((props.local_com - Vec3::new(-0.5, 0.5, 0.0)).length() < 1e-4);
    let inertia = props.principal_inertia();
    let (mut i, want) = (inertia.to_array(), [6.0 * 0.7, 37.0 * 0.7, 41.0 * 0.7]);
    i.sort_by(f32::total_cmp);
    for (a, b) in i.iter().zip(want) {
        assert!((a - b).abs() < 1e-3, "{inertia}");
    }
}

/// Versions without saved mass weigh what their hulls hold.
#[test]
fn hulls_weigh_their_volume() {
    let mut dom = Dom::new();
    let u = shaped(&mut dom, "MeshPart", Vec3::ZERO, true);
    saved(&mut dom, u, gap());
    let physics = Physics::new();
    assert!((physics.part_mass(&dom, u) - 16.0 * 0.7).abs() < 1e-4);
}

#[test]
fn parts_fall_through_a_unions_gaps() {
    let mut dom = Dom::new();
    let flat = union(&mut dom, Vec3::ZERO, true);
    let boxed = union(&mut dom, Vec3::new(20.0, 0.0, 0.0), true);
    let mut physics = Physics::new();
    saved(&mut dom, flat, gap());
    let through = part(&mut dom, O + Vec3::new(0.0, 3.0, 0.0), Vec3::ONE, false);
    let on = part(&mut dom, O + Vec3::new(20.0, 3.0, 0.0), Vec3::ONE, false);
    run(&mut physics, &mut dom, 1.0);
    assert!(
        position(&dom, through).y < O.y - 5.0,
        "{:?}",
        position(&dom, through)
    );
    assert!((position(&dom, on).y - (O.y + 1.5)).abs() < 0.05);
    let _ = boxed;
}

/// A moving part that doesn't collide keeps its box (for speed).
#[test]
fn moving_non_colliding_parts_keep_their_box() {
    let mut dom = Dom::new();
    let u = union(&mut dom, Vec3::ZERO, false);
    set(&mut dom, u, "CanCollide", Value::Bool(false));
    let mut physics = Physics::new();
    saved(&mut dom, u, gap());
    assert_eq!(ray_at(&mut physics, &mut dom, 0.0), Some(4.0));
}
