use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, Value, Vector3};

use super::*;

mod constraints;
mod contacts;
mod forces;
mod joints;
mod make_joints;
mod movers;
mod queries;
mod unions;

fn set(dom: &mut Dom, id: InstanceId, name: &str, value: Value) {
    dom.set_property_by_name(id, name, value).unwrap();
}

/// A new `class` under `parent` with `props` set first.
fn child(dom: &mut Dom, class: &str, parent: InstanceId, props: &[(&str, Value)]) -> InstanceId {
    let id = dom.create_by_name(class).unwrap();
    for (name, value) in props {
        set(dom, id, name, value.clone());
    }
    dom.set_parent(id, Some(parent)).unwrap();
    id
}

fn v3(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3(Vec3::new(x, y, z)))
}

fn vector(dom: &Dom, id: InstanceId, name: &str) -> Vec3 {
    dom.property_by_name(id, name)
        .and_then(|v| v.as_vector3())
        .unwrap()
}

fn cframe(dom: &Dom, id: InstanceId) -> CFrame {
    dom.property_by_name(id, "CFrame")
        .and_then(|v| v.as_cframe())
        .unwrap()
}

fn part(dom: &mut Dom, class: &str, position: Vec3, size: Vec3, anchored: bool) -> InstanceId {
    let ws = dom.get_service("Workspace").unwrap();
    let id = dom.create_by_name(class).unwrap();
    set(
        dom,
        id,
        "CFrame",
        Value::CFrame(CFrame::from_position(position)),
    );
    set(dom, id, "Size", Value::Vector3(Vector3(size)));
    set(dom, id, "Anchored", Value::Bool(anchored));
    dom.set_parent(id, Some(ws)).unwrap();
    id
}

fn position(dom: &Dom, id: InstanceId) -> Vec3 {
    match dom.property_by_name(id, "CFrame") {
        Some(Value::CFrame(cf)) => cf.position,
        other => panic!("{other:?}"),
    }
}

/// Step `seconds` at 60 Hz, the way a DataModel does.
fn run(physics: &mut Physics, dom: &mut Dom, seconds: f64) -> Vec<InstanceId> {
    let mut fallen = Vec::new();
    for _ in 0..(seconds * 60.0).round() as u32 {
        physics.sync(dom);
        physics.step(1.0 / 60.0);
        fallen.extend(physics.write_back(dom));
    }
    fallen
}

#[test]
fn properties_resolve() {
    let dom = Dom::new();
    assert!(Props::resolve(&dom).is_some());
}

#[test]
fn unanchored_part_falls_and_lands_on_an_anchored_one() {
    let mut dom = Dom::new();
    let base = part(
        &mut dom,
        "Part",
        Vec3::ZERO,
        Vec3::new(64.0, 2.0, 64.0),
        true,
    );
    let brick = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 20.0, 0.0),
        Vec3::new(4.0, 2.0, 4.0),
        false,
    );
    let mut physics = Physics::new();
    physics.sync(&mut dom);
    assert_eq!(physics.part_count(), 2);
    assert_eq!(physics.body_count(), 1);

    // Free fall at 196.2 studs/s²: after 0.25 s it has dropped about 6.1.
    run(&mut physics, &mut dom, 0.25);
    let y = position(&dom, brick).y;
    assert!((y - (20.0 - 0.5 * 196.2 * 0.0625)).abs() < 0.3, "{y}");

    // Resting on the baseplate: its centre is 1 + 1 studs above the base's.
    run(&mut physics, &mut dom, 3.0);
    let y = position(&dom, brick).y;
    assert!((y - 2.0).abs() < 0.1, "{y}");
    assert_eq!(position(&dom, base), Vec3::ZERO);
}

#[test]
fn script_changes_reach_the_world() {
    let mut dom = Dom::new();
    let brick = part(&mut dom, "Part", Vec3::new(0.0, 50.0, 0.0), Vec3::ONE, true);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.5);
    assert_eq!(position(&dom, brick).y, 50.0, "anchored parts stay put");

    set(&mut dom, brick, "Anchored", Value::Bool(false));
    run(&mut physics, &mut dom, 0.5);
    assert!(position(&dom, brick).y < 30.0);

    // Moving it out of Workspace takes it out of the simulation.
    let storage = dom.get_service("ReplicatedStorage").unwrap();
    dom.set_parent(brick, Some(storage)).unwrap();
    physics.sync(&mut dom);
    assert_eq!(physics.part_count(), 0);
}

#[test]
fn workspace_gravity_is_used() {
    let mut dom = Dom::new();
    let brick = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 50.0, 0.0),
        Vec3::ONE,
        false,
    );
    let ws = dom.find_service("Workspace").unwrap();
    set(&mut dom, ws, "Gravity", Value::Float32(0.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.5);
    assert_eq!(position(&dom, brick).y, 50.0);
}

#[test]
fn parts_below_the_destroy_height_are_reported() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    set(
        &mut dom,
        ws,
        "FallenPartsDestroyHeight",
        Value::Float32(-10.0),
    );
    let brick = part(&mut dom, "Part", Vec3::ZERO, Vec3::ONE, false);
    let mut physics = Physics::new();
    let fallen = run(&mut physics, &mut dom, 1.0);
    assert!(fallen.contains(&brick));
}

#[test]
fn shapes_have_the_right_extent() {
    // A cylinder lies along X, a ball's radius is half the smallest side.
    let size = Vec3::new(6.0, 2.0, 2.0);
    let (cyl, offset) = shape::collider_shape(PartShape::Cylinder, size).unwrap();
    let aabb = cyl.compute_aabb(&offset);
    assert!((aabb.maxs.x - 3.0).abs() < 1e-4 && (aabb.maxs.y - 1.0).abs() < 1e-4);
    let (ball, _) = shape::collider_shape(PartShape::Ball, size).unwrap();
    assert!((ball.as_ball().unwrap().radius - 1.0).abs() < 1e-6);
    // A wedge's top edge is at the back (+Z).
    let (wedge, offset) = shape::collider_shape(PartShape::Wedge, Vec3::splat(2.0)).unwrap();
    let aabb = wedge.compute_aabb(&offset);
    assert_eq!((aabb.mins.y, aabb.maxs.y), (-1.0, 1.0));
}
