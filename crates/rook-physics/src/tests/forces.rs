//! BodyForce, BodyThrust, RocketPropulsion, VectorForce, Torque and part
//! velocities. Expected values were observed in Studio 0.742 with 2×2×2
//! plastic parts (mass 5.6).

use glam::Vec3;
use rook_dom::Dom;
use rook_types::{CFrame, Value};

use super::movers::{cube, hold, spin, velocity};
use super::{block, child, part, position, run, set, v3};
use crate::{Command, Physics};

#[test]
fn body_force_and_thrust() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    set(&mut dom, ws, "Gravity", Value::Float32(0.0));
    let f = cube(&mut dom, 0.0);
    child(&mut dom, "BodyForce", f, &[("Force", v3(56.0, 0.0, 0.0))]);
    // A thrust's force is in the part's frame: turned 90° about Y, its X
    // points along world -Z. Off-center it also turns the part.
    let t = cube(&mut dom, 20.0);
    set(
        &mut dom,
        t,
        "CFrame",
        Value::CFrame(
            CFrame::from_position(Vec3::new(20.0, 1000.0, 0.0))
                * CFrame::angles(0.0, std::f32::consts::FRAC_PI_2, 0.0),
        ),
    );
    child(&mut dom, "BodyThrust", t, &[("Force", v3(56.0, 0.0, 0.0))]);
    let r = cube(&mut dom, 40.0);
    set(
        &mut dom,
        r,
        "CFrame",
        Value::CFrame(
            CFrame::from_position(Vec3::new(40.0, 1000.0, 0.0))
                * CFrame::angles(0.0, std::f32::consts::FRAC_PI_2, 0.0),
        ),
    );
    hold(&mut dom, r);
    child(
        &mut dom,
        "BodyThrust",
        r,
        &[
            ("Force", v3(0.0, 0.0, 3.7333)),
            ("Location", v3(0.0, 1.0, 0.0)),
        ],
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.5);
    assert!((velocity(&dom, f) - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-3);
    assert!((velocity(&dom, t) - Vec3::new(0.0, 0.0, -5.0)).length() < 1e-3);
    // Observed: -1 rad/s² about world Z.
    let w = spin(&dom, r);
    assert!((w - Vec3::new(0.0, 0.0, -0.5)).length() < 0.02, "{w}");
}

#[test]
fn movers_act_on_the_whole_assembly() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    set(&mut dom, ws, "Gravity", Value::Float32(0.0));
    let a = part(
        &mut dom,
        Vec3::new(0.0, 1000.0, 0.0),
        Vec3::splat(4.0),
        false,
    );
    let b = block(&mut dom, Vec3::new(3.0, 1000.0, 0.0), false);
    child(
        &mut dom,
        "WeldConstraint",
        a,
        &[
            ("Part0", Value::Ref(Some(a.to_ref()))),
            ("Part1", Value::Ref(Some(b.to_ref()))),
        ],
    );
    // Total mass 50.4: 1 stud/s² (observed), and no spin.
    child(
        &mut dom,
        "BodyVelocity",
        b,
        &[
            ("Velocity", v3(100.0, 0.0, 0.0)),
            ("MaxForce", v3(50.4, 0.0, 0.0)),
        ],
    );
    // Anchored parts don't move.
    let c = part(&mut dom, Vec3::new(20.0, 1000.0, 0.0), Vec3::ONE, true);
    child(
        &mut dom,
        "BodyPosition",
        c,
        &[("Position", v3(30.0, 1000.0, 0.0))],
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.5);
    assert!((velocity(&dom, a).x - 0.5).abs() < 0.01);
    assert!(spin(&dom, a).length() < 1e-3);
    assert_eq!(position(&dom, c), Vec3::new(20.0, 1000.0, 0.0));
}

#[test]
fn rocket_flies_to_its_target_once_fired() {
    let mut dom = Dom::new();
    let target = part(&mut dom, Vec3::new(100.0, 1000.0, 0.0), Vec3::ONE, true);
    let p = cube(&mut dom, 0.0);
    let rocket = child(
        &mut dom,
        "RocketPropulsion",
        p,
        &[("Target", Value::Ref(Some(target.to_ref())))],
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.1);
    assert!(position(&dom, p).y < 999.5, "falls until fired");
    let start = position(&dom, p);

    physics.command(Command::FireRocket(rocket));
    run(&mut physics, &mut dom, 0.5);
    // Straight at the target at MaxSpeed (observed 28.8 to 30), holding
    // against gravity.
    let v = velocity(&dom, p);
    let dir = (Vec3::new(100.0, 1000.0, 0.0) - position(&dom, p)).normalize();
    assert!((v.length() - 30.0).abs() < 1.5, "{v}");
    assert!(v.normalize().dot(dir) > 0.999, "{v} {dir}");
    assert!(position(&dom, p).y > start.y - 0.5);
    assert!(physics.take_reached().is_empty());

    run(&mut physics, &mut dom, 3.5);
    assert_eq!(physics.take_reached(), vec![rocket]);
    let at = position(&dom, p);
    assert!(at.distance(Vec3::new(100.0, 1000.0, 0.0)) <= 4.0, "{at}");
    // It holds there until aborted.
    run(&mut physics, &mut dom, 0.5);
    assert!(velocity(&dom, p).length() < 0.5);
    physics.command(Command::AbortRocket(rocket));
    run(&mut physics, &mut dom, 0.5);
    assert!(velocity(&dom, p).y < -90.0);
}

#[test]
fn rocket_faces_between_target_and_thrust() {
    // Observed: CartoonFactor 0 under gravity points it up (its thrust),
    // 1 points it at the target.
    for (cartoon, up) in [(0.0, true), (1.0, false)] {
        let mut dom = Dom::new();
        let target = part(&mut dom, Vec3::new(100.0, 1000.0, 0.0), Vec3::ONE, true);
        let p = cube(&mut dom, 0.0);
        let rocket = child(
            &mut dom,
            "RocketPropulsion",
            p,
            &[
                ("Target", Value::Ref(Some(target.to_ref()))),
                ("CartoonFactor", Value::Float32(cartoon)),
            ],
        );
        let mut physics = Physics::new();
        physics.command(Command::FireRocket(rocket));
        run(&mut physics, &mut dom, 1.5);
        let look = -dom.cframe_property(p, "CFrame").unwrap().rotation.z_axis;
        if up {
            assert!(look.y > 0.95, "{look}");
        } else {
            assert!(look.x > 0.98, "{look}");
        }
    }
}

#[test]
fn vector_force_and_torque() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    set(&mut dom, ws, "Gravity", Value::Float32(0.0));
    let p = cube(&mut dom, 0.0);
    set(
        &mut dom,
        p,
        "CFrame",
        Value::CFrame(
            CFrame::from_position(Vec3::new(0.0, 1000.0, 0.0))
                * CFrame::angles(0.0, std::f32::consts::FRAC_PI_2, 0.0),
        ),
    );
    let a = child(&mut dom, "Attachment", p, &[]);
    // Relative to Attachment0 (the default) and at the center of mass.
    child(
        &mut dom,
        "VectorForce",
        p,
        &[
            ("Attachment0", Value::Ref(Some(a.to_ref()))),
            ("Force", v3(5.6, 0.0, 0.0)),
            ("ApplyAtCenterOfMass", Value::Bool(true)),
        ],
    );
    // Observed: Torque 10 world Y spins a 2-stud cube at 2.68 rad/s².
    child(
        &mut dom,
        "Torque",
        p,
        &[
            ("Attachment0", Value::Ref(Some(a.to_ref()))),
            ("Torque", v3(0.0, 10.0, 0.0)),
            ("RelativeTo", Value::Enum(2)),
        ],
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.25);
    let v = velocity(&dom, p);
    assert!(v.z < -0.2 && v.x.abs() < 0.05, "{v}");
    let w = spin(&dom, p).y / 0.25;
    assert!((w - 2.68).abs() < 0.05, "{w}");
}

#[test]
fn anchored_parts_with_velocity_are_conveyors() {
    // Observed: a belt with Velocity 10 carries a box on it at 10.
    let mut dom = Dom::new();
    let belt = part(
        &mut dom,
        Vec3::new(0.0, 1000.0, 0.0),
        Vec3::new(20.0, 1.0, 4.0),
        true,
    );
    set(&mut dom, belt, "AssemblyLinearVelocity", v3(10.0, 0.0, 0.0));
    let boxed = block(&mut dom, Vec3::new(-5.0, 1001.5, 0.0), false);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 1.0);
    let v = velocity(&dom, boxed);
    assert!((v.x - 10.0).abs() < 0.2 && v.z.abs() < 0.1, "{v}");
    assert_eq!(position(&dom, belt), Vec3::new(0.0, 1000.0, 0.0));
}

#[test]
fn velocity_is_the_assembly_velocity_at_the_part() {
    // Observed: a 2-stud cube welded 4 studs from a 4-stud cube's center,
    // the assembly spinning at 2 rad/s about Y, reads (0, 0, -7.11).
    let mut dom = Dom::new();
    let root = part(
        &mut dom,
        Vec3::new(0.0, 1000.0, 0.0),
        Vec3::splat(4.0),
        false,
    );
    let arm = block(&mut dom, Vec3::new(4.0, 1000.0, 0.0), false);
    child(
        &mut dom,
        "WeldConstraint",
        root,
        &[
            ("Part0", Value::Ref(Some(root.to_ref()))),
            ("Part1", Value::Ref(Some(arm.to_ref()))),
        ],
    );
    for p in [root, arm] {
        set(&mut dom, p, "AssemblyAngularVelocity", v3(0.0, 2.0, 0.0));
    }
    let v = crate::assembly::part_velocity(&dom, arm);
    assert!((v - Vec3::new(0.0, 0.0, -7.111)).length() < 0.01, "{v}");
}

#[test]
fn saved_velocities_are_adopted() {
    let mut dom = Dom::new();
    let p = cube(&mut dom, 0.0);
    set(&mut dom, p, "Velocity", v3(50.0, 0.0, 0.0));
    set(&mut dom, p, "RotVelocity", v3(0.0, 3.0, 0.0));
    crate::adopt_saved_velocities(&mut dom);
    assert_eq!(velocity(&dom, p), Vec3::new(50.0, 0.0, 0.0));
    assert_eq!(spin(&dom, p), Vec3::new(0.0, 3.0, 0.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.1);
    assert!((position(&dom, p).x - 5.0).abs() < 0.1);
}
