//! BodyVelocity, BodyPosition and BodyGyro. Expected values were observed
//! in Studio 0.742 with 2×2×2 plastic parts (mass 5.6).

use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, Value};

use super::{block, child, part, position, run, set, v3};
use crate::Physics;

pub(super) fn cube(dom: &mut Dom, x: f32) -> InstanceId {
    block(dom, Vec3::new(x, 1000.0, 0.0), false)
}

pub(super) fn velocity(dom: &Dom, id: InstanceId) -> Vec3 {
    dom.vector3_property(id, "AssemblyLinearVelocity").unwrap()
}

pub(super) fn spin(dom: &Dom, id: InstanceId) -> Vec3 {
    dom.vector3_property(id, "AssemblyAngularVelocity").unwrap()
}

/// Hold a part still on every axis, as the gyro measurements did.
pub(super) fn hold(dom: &mut Dom, p: InstanceId) {
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
    let (_, y, _) = dom.cframe_property(p, "CFrame").unwrap().to_orientation();
    assert!((y.to_degrees() - 0.58f32.to_degrees()).abs() < 3.0, "{y}");
    run(&mut physics, &mut dom, 3.0);
    assert!(
        dom.cframe_property(p, "CFrame")
            .unwrap()
            .rotation
            .abs_diff_eq(target.rotation, 0.01)
    );
}

#[test]
fn movers_holding_still_let_their_body_sleep() {
    let mut dom = Dom::new();
    part(&mut dom, Vec3::ZERO, Vec3::new(64.0, 2.0, 64.0), true);
    let p = block(&mut dom, Vec3::new(0.0, 2.0, 0.0), false);
    let gyro = child(&mut dom, "BodyGyro", p, &[("MaxTorque", v3(1e9, 1e9, 1e9))]);
    child(
        &mut dom,
        "BodyVelocity",
        p,
        &[("Velocity", v3(0.0, 0.0, 0.0))],
    );
    let mut physics = Physics::new();
    let sleeping = |physics: &Physics| physics.world.bodies[physics.parts[&p]].is_sleeping();
    // Observed: a gyro at its target and a zero BodyVelocity sleep.
    run(&mut physics, &mut dom, 3.0);
    assert!(sleeping(&physics));
    // Changing a mover wakes its body, which turns to the new target.
    let target = CFrame::angles(0.0, 0.5, 0.0);
    set(&mut dom, gyro, "CFrame", Value::CFrame(target));
    run(&mut physics, &mut dom, 1.0 / 60.0);
    assert!(!sleeping(&physics));
    run(&mut physics, &mut dom, 3.0);
    assert!(
        dom.cframe_property(p, "CFrame")
            .unwrap()
            .rotation
            .abs_diff_eq(target.rotation, 0.01)
    );
}
