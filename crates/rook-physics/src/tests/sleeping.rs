//! When assemblies sleep. Expected values were observed in Studio 0.745
//! (docs/behavior/physics/sleeping.md).

use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{CustomPhysicalProperties, PhysicalProperties, Value};

use super::{block, child, floor, run, set, v3};
use crate::Physics;

fn frictionless(dom: &mut Dom, id: InstanceId) {
    let c = CustomPhysicalProperties::new(0.7, 0.0, 0.0, 100.0, 1.0, 1.0);
    let value = Value::PhysicalProperties(PhysicalProperties::Custom(c));
    set(dom, id, "CustomPhysicalProperties", value);
}

/// How fast a frictionless block set sliding at `speed` studs/s on a
/// frictionless floor slides a second and a half later.
fn slid(speed: f32) -> f32 {
    let mut dom = Dom::new();
    let ground = floor(&mut dom);
    let b = block(&mut dom, Vec3::new(0.0, 1.5, 0.0), false);
    frictionless(&mut dom, ground);
    frictionless(&mut dom, b);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 1.0);
    let v = Value::Vector3(rook_types::Vector3(Vec3::new(speed, 0.0, 0.0)));
    set(&mut dom, b, "AssemblyLinearVelocity", v);
    run(&mut physics, &mut dom, 1.5);
    dom.vector3_property(b, "AssemblyLinearVelocity")
        .expect("a velocity")
        .x
}

/// A block sliding at 0.32 studs/s or less stops dead after half a
/// second; at 0.34 or more it slides on.
#[test]
fn slow_sliders_sleep_as_in_studio() {
    assert_eq!(slid(0.3), 0.0, "asleep");
    let on = slid(0.4);
    assert!((on - 0.4).abs() < 0.02, "slides on at {on}");
}

/// Whether a block drifting at 0.3 studs/s with gravity off, pushed
/// against its motion at `accel` studs/s², is ever at rest within 1.5 s.
fn pushed_sleeps(accel: f32) -> bool {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    set(&mut dom, ws, "Gravity", Value::Float32(0.0));
    let b = block(&mut dom, Vec3::new(0.0, 1000.0, 0.0), false);
    let a = child(&mut dom, "Attachment", b, &[]);
    // A 2-stud block of plastic weighs 5.6; RelativeTo World.
    let force = [
        ("Attachment0", Value::Ref(Some(a.to_ref()))),
        ("Force", v3(-5.6 * accel, 0.0, 0.0)),
        ("ApplyAtCenterOfMass", Value::Bool(true)),
        ("RelativeTo", Value::Enum(2)),
    ];
    child(&mut dom, "VectorForce", b, &force);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.1);
    set(&mut dom, b, "AssemblyLinearVelocity", v3(0.3, 0.0, 0.0));
    (0..15).any(|_| {
        run(&mut physics, &mut dom, 0.1);
        dom.vector3_property(b, "AssemblyLinearVelocity") == Some(Vec3::ZERO)
    })
}

/// However slow, an assembly speeding up or slowing down at 0.25 studs/s²
/// or more stays awake; at 0.24 or less it sleeps.
#[test]
fn accelerating_assemblies_stay_awake_as_in_studio() {
    assert!(pushed_sleeps(0.2), "asleep");
    assert!(!pushed_sleeps(0.3), "awake");
}
