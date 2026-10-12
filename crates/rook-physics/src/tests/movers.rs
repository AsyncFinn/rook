//! BodyMovers, RocketPropulsion, VectorForce and Torque. Expected values
//! were observed in Studio 0.742 with 2×2×2 plastic parts (mass 5.6).

use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, Value};

use super::{cframe, child, part, position, run, set, v3, vector};
use crate::{Command, Physics};

fn cube(dom: &mut Dom, x: f32) -> InstanceId {
    part(
        dom,
        "Part",
        Vec3::new(x, 1000.0, 0.0),
        Vec3::splat(2.0),
        false,
    )
}

fn velocity(dom: &Dom, id: InstanceId) -> Vec3 {
    vector(dom, id, "AssemblyLinearVelocity")
}

fn spin(dom: &Dom, id: InstanceId) -> Vec3 {
    vector(dom, id, "AssemblyAngularVelocity")
}

/// Hold a part still on every axis, as the gyro measurements did.
fn hold(dom: &mut Dom, p: InstanceId) {
    let at = position(dom, p);
    child(
        dom,
        "BodyPosition",
        p,
        &[
            ("Position", v3(at.x, at.y, at.z)),
            ("MaxForce", v3(1e9, 1e9, 1e9)),
        ],
    );
}

#[test]
fn body_velocity_is_a_velocity_constraint() {
    let mut dom = Dom::new();
    let fast = cube(&mut dom, 0.0);
    let slow = cube(&mut dom, 20.0);
    for (p, max) in [(fast, 1e9), (slow, 40.0)] {
        child(
            &mut dom,
            "BodyVelocity",
            p,
            &[
                ("Velocity", v3(10.0, 0.0, 0.0)),
                ("MaxForce", v3(max, 0.0, 0.0)),
                ("P", Value::Float32(10.0)),
            ],
        );
    }
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 1.0 / 60.0);
    // Observed: 10 within the first frame, whatever P.
    assert!((velocity(&dom, fast).x - 10.0).abs() < 1e-3);
    // MaxForce 40 on 5.6: 7.14 studs/s² (observed 0.119 per frame).
    run(&mut physics, &mut dom, 0.5);
    let v = velocity(&dom, slow).x;
    assert!((v - 40.0 / 5.6 * (0.5 + 1.0 / 60.0)).abs() < 0.05, "{v}");
    // No force allowed on Y: it falls.
    assert!(velocity(&dom, fast).y < -90.0);
}

#[test]
fn body_velocity_holds_against_gravity_on_axes_it_may_push() {
    let mut dom = Dom::new();
    let p = cube(&mut dom, 0.0);
    child(
        &mut dom,
        "BodyVelocity",
        p,
        &[
            ("Velocity", v3(0.0, 0.0, 0.0)),
            ("MaxForce", v3(4000.0, 4000.0, 4000.0)),
        ],
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 1.0);
    assert!((position(&dom, p).y - 1000.0).abs() < 0.01);
}

#[test]
fn body_position_is_a_mass_independent_spring() {
    // Observed: P 100, D 10 overshoots to 11.3 near 0.33 s and settles,
    // the same for a cube 4 studs wide.
    for size in [2.0, 4.0] {
        let mut dom = Dom::new();
        let p = part(
            &mut dom,
            "Part",
            Vec3::new(0.0, 1000.0, 0.0),
            Vec3::splat(size),
            false,
        );
        child(
            &mut dom,
            "BodyPosition",
            p,
            &[
                ("Position", v3(10.0, 1000.0, 0.0)),
                ("P", Value::Float32(100.0)),
                ("D", Value::Float32(10.0)),
                ("MaxForce", v3(1e9, 1e9, 0.0)),
            ],
        );
        let mut physics = Physics::new();
        let mut peak: f32 = 0.0;
        for _ in 0..40 {
            run(&mut physics, &mut dom, 1.0 / 60.0);
            peak = peak.max(position(&dom, p).x);
        }
        // Rook overshoots a little more (11.6): Roblox damps slightly
        // more than a damper of D at low D (Approximation).
        assert!((peak - 11.3).abs() < 0.35, "size {size}: peak {peak}");
        run(&mut physics, &mut dom, 2.0);
        assert!((position(&dom, p).x - 10.0).abs() < 0.05);
    }
}

#[test]
fn body_position_damping_grows_with_its_square() {
    // Observed with P 10000: settled speed / distance is 2.47 for D 1250
    // (the default) and 0.615 for D 2500.
    for (d, ratio) in [(1250.0, 2.47), (2500.0, 0.615)] {
        let mut dom = Dom::new();
        let p = cube(&mut dom, 0.0);
        child(
            &mut dom,
            "BodyPosition",
            p,
            &[
                ("Position", v3(100.0, 1000.0, 0.0)),
                ("P", Value::Float32(10000.0)),
                ("D", Value::Float32(d)),
                ("MaxForce", v3(1e12, 1e12, 1e12)),
            ],
        );
        let mut physics = Physics::new();
        run(&mut physics, &mut dom, 0.2);
        let r = velocity(&dom, p).x / (100.0 - position(&dom, p).x);
        assert!((r - ratio).abs() < ratio * 0.05, "D {d}: {r}");
    }
}

#[test]
fn body_position_sags_only_by_gravity_over_p() {
    let mut dom = Dom::new();
    let p = cube(&mut dom, 0.0);
    child(
        &mut dom,
        "BodyPosition",
        p,
        &[("Position", v3(0.0, 1005.0, 0.0))],
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 4.0);
    // Observed: on target to within 0.05 (196.2 / P is 0.02).
    let y = position(&dom, p).y;
    assert!((y - 1005.0).abs() < 0.05, "{y}");
}

#[test]
fn body_position_force_is_limited() {
    let mut dom = Dom::new();
    let p = cube(&mut dom, 0.0);
    child(
        &mut dom,
        "BodyPosition",
        p,
        &[
            ("Position", v3(10.0, 1000.0, 0.0)),
            ("MaxForce", v3(40.0, 1e9, 0.0)),
        ],
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.2);
    let v = velocity(&dom, p).x;
    assert!((v - 40.0 / 5.6 * 0.2).abs() < 0.03, "{v}");
}

#[test]
fn body_gyro_limits_angular_acceleration_not_torque() {
    // Observed: MaxTorque 2 speeds both a 2- and a 4-stud cube up by
    // 2 rad/s², and P acts on the sine of the angle at a quarter strength.
    for size in [2.0, 4.0] {
        let mut dom = Dom::new();
        let p = part(
            &mut dom,
            "Part",
            Vec3::new(0.0, 1000.0, 0.0),
            Vec3::splat(size),
            false,
        );
        hold(&mut dom, p);
        child(
            &mut dom,
            "BodyGyro",
            p,
            &[
                ("CFrame", Value::CFrame(CFrame::angles(0.0, 3.0, 0.0))),
                ("MaxTorque", v3(0.0, 2.0, 0.0)),
                ("P", Value::Float32(1e6)),
                ("D", Value::Float32(0.0)),
            ],
        );
        let mut physics = Physics::new();
        run(&mut physics, &mut dom, 8.0 / 60.0);
        let w = spin(&dom, p).y;
        assert!((w - 0.266).abs() < 0.01, "size {size}: {w}");
    }
}

#[test]
fn body_gyro_turns_on_the_sine_of_its_error() {
    let mut dom = Dom::new();
    let p = cube(&mut dom, 0.0);
    hold(&mut dom, p);
    child(
        &mut dom,
        "BodyGyro",
        p,
        &[
            ("CFrame", Value::CFrame(CFrame::angles(0.0, 1.0, 0.0))),
            ("MaxTorque", v3(1e9, 1e9, 1e9)),
            ("P", Value::Float32(4.0)),
            ("D", Value::Float32(0.0)),
        ],
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.25);
    // Observed: 4/4 · sin(1) = 0.84 rad/s².
    let alpha = spin(&dom, p).y / 0.25;
    assert!((alpha - 0.84).abs() < 0.03, "{alpha}");
}

#[test]
fn body_gyro_settles_with_default_gains() {
    let mut dom = Dom::new();
    let p = cube(&mut dom, 0.0);
    hold(&mut dom, p);
    let target = CFrame::angles(0.0, 1.0, 0.0);
    child(
        &mut dom,
        "BodyGyro",
        p,
        &[
            ("CFrame", Value::CFrame(target)),
            ("MaxTorque", v3(1e9, 1e9, 1e9)),
        ],
    );
    let mut physics = Physics::new();
    // Observed: 0.58 of the way after 12 frames.
    run(&mut physics, &mut dom, 0.2);
    let (_, y, _) = cframe(&dom, p).to_orientation();
    assert!((y.to_degrees() - 0.58f32.to_degrees()).abs() < 3.0, "{y}");
    run(&mut physics, &mut dom, 3.0);
    assert!(cframe(&dom, p).rotation.abs_diff_eq(target.rotation, 0.01));
}

#[test]
fn body_force_and_thrust() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    set(&mut dom, ws, "Gravity", Value::Float32(0.0));
    let f = cube(&mut dom, 0.0);
    child(&mut dom, "BodyForce", f, &[("Force", v3(56.0, 0.0, 0.0))]);
    // A thrust's force is in the part's frame: turned 90° about Y, its X
    // points along world -Z. Off-centre it also turns the part.
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
        "Part",
        Vec3::new(0.0, 1000.0, 0.0),
        Vec3::splat(4.0),
        false,
    );
    let b = part(
        &mut dom,
        "Part",
        Vec3::new(3.0, 1000.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
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
    let c = part(
        &mut dom,
        "Part",
        Vec3::new(20.0, 1000.0, 0.0),
        Vec3::ONE,
        true,
    );
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
    let target = part(
        &mut dom,
        "Part",
        Vec3::new(100.0, 1000.0, 0.0),
        Vec3::ONE,
        true,
    );
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
        let target = part(
            &mut dom,
            "Part",
            Vec3::new(100.0, 1000.0, 0.0),
            Vec3::ONE,
            true,
        );
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
        let look = -cframe(&dom, p).rotation.z_axis;
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
    // Relative to Attachment0 (the default) and at the centre of mass.
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
        "Part",
        Vec3::new(0.0, 1000.0, 0.0),
        Vec3::new(20.0, 1.0, 4.0),
        true,
    );
    set(&mut dom, belt, "AssemblyLinearVelocity", v3(10.0, 0.0, 0.0));
    let boxed = part(
        &mut dom,
        "Part",
        Vec3::new(-5.0, 1001.5, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 1.0);
    let v = velocity(&dom, boxed);
    assert!((v.x - 10.0).abs() < 0.2 && v.z.abs() < 0.1, "{v}");
    assert_eq!(position(&dom, belt), Vec3::new(0.0, 1000.0, 0.0));
}

#[test]
fn velocity_is_the_assembly_velocity_at_the_part() {
    // Observed: a 2-stud cube welded 4 studs from a 4-stud cube's centre,
    // the assembly spinning at 2 rad/s about Y, reads (0, 0, -7.11).
    let mut dom = Dom::new();
    let root = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 1000.0, 0.0),
        Vec3::splat(4.0),
        false,
    );
    let arm = part(
        &mut dom,
        "Part",
        Vec3::new(4.0, 1000.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
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
