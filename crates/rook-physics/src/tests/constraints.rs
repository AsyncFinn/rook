//! Attachment constraints and driven hinges (HingeConstraint actuators,
//! RotateV, RotateP). Expected values were observed in Studio 0.742.

use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, Value};

use super::{cframe, child, part, position, run, set, v3, vector};
use crate::Physics;

/// A wheel's turn about world Y relative to the base.
fn yaw(dom: &Dom, base: InstanceId, wheel: InstanceId) -> f32 {
    let rel = cframe(dom, base).inverse() * cframe(dom, wheel);
    let (_, y, _) = rel.to_orientation();
    y
}

fn float(dom: &Dom, id: InstanceId, name: &str) -> f32 {
    match dom.property_by_name(id, name) {
        Some(Value::Float32(f)) => f,
        other => panic!("{other:?}"),
    }
}

fn smooth(dom: &mut Dom, p: InstanceId) {
    for face in ["Top", "Bottom", "Left", "Right", "Front", "Back"] {
        set(dom, p, &format!("{face}Surface"), Value::Enum(0));
    }
}

/// An anchored base with a 2-stud cube resting on it, joined by
/// MakeJoints through the base's top surface of type `surface`.
fn surface_hinge(
    dom: &mut Dom,
    surface: u32,
    input: u32,
    a: f32,
    b: f32,
) -> (InstanceId, InstanceId) {
    let base = part(
        dom,
        "Part",
        Vec3::new(0.0, 1000.0, 0.0),
        Vec3::new(4.0, 1.0, 4.0),
        true,
    );
    let wheel = part(
        dom,
        "Part",
        Vec3::new(0.0, 1001.5, 0.0),
        Vec3::splat(2.0),
        false,
    );
    smooth(dom, base);
    smooth(dom, wheel);
    set(dom, base, "TopSurface", Value::Enum(surface));
    set(dom, base, "TopSurfaceInput", Value::Enum(input));
    set(dom, base, "TopParamA", Value::Float32(a));
    set(dom, base, "TopParamB", Value::Float32(b));
    crate::surfaces::make_joints(dom, &[wheel]).unwrap();
    (base, wheel)
}

const MOTOR: u32 = 7;
const STEPPING_MOTOR: u32 = 8;
const NO_INPUT: u32 = 0;
const CONSTANT: u32 = 12;
const SIN: u32 = 13;

#[test]
fn rotate_v_turns_at_31_per_unit_of_param_b() {
    for b in [0.1, 1.0] {
        let mut dom = Dom::new();
        let (_, wheel) = surface_hinge(&mut dom, MOTOR, CONSTANT, 0.0, b);
        let mut physics = Physics::new();
        run(&mut physics, &mut dom, 0.25);
        let w = vector(&dom, wheel, "AssemblyAngularVelocity");
        assert!((w.y - 31.0 * b).abs() < 0.05 * 31.0 * b, "B {b}: {w}");
    }
}

/// A 40 × 1 × 4 bar (inertia 15,083) on an anchored cube of side `motor`
/// whose top is a Motor surface at ParamB `b`.
fn rotor_on_motor(dom: &mut Dom, motor: f32, b: f32) -> InstanceId {
    let base = part(
        dom,
        "Part",
        Vec3::new(0.0, 1000.0, 0.0),
        Vec3::splat(motor),
        true,
    );
    let bar = part(
        dom,
        "Part",
        Vec3::new(0.0, 1000.5 + motor / 2.0, 0.0),
        Vec3::new(40.0, 1.0, 4.0),
        false,
    );
    smooth(dom, base);
    smooth(dom, bar);
    set(dom, base, "TopSurface", Value::Enum(MOTOR));
    set(dom, base, "TopSurfaceInput", Value::Enum(CONSTANT));
    set(dom, base, "TopParamB", Value::Float32(b));
    crate::surfaces::make_joints(dom, &[bar]).unwrap();
    bar
}

#[test]
fn rotate_v_torque_is_limited_by_the_smaller_part() {
    // Observed: on a 2-stud cube at ParamB 10 the bar gains 1,880 rad/s²
    // until it reaches 310; on a 1-stud cube, 32 times less.
    let spin = |motor, seconds| {
        let mut dom = Dom::new();
        let bar = rotor_on_motor(&mut dom, motor, 10.0);
        let mut physics = Physics::new();
        run(&mut physics, &mut dom, seconds);
        vector(&dom, bar, "AssemblyAngularVelocity").y
    };
    let w = spin(2.0, 0.1);
    assert!((w - 178.0).abs() < 25.0, "{w}");
    let w = spin(2.0, 0.5);
    assert!((w - 310.0).abs() < 1.0, "{w}");
    let w = spin(1.0, 1.0);
    assert!((w - 55.0).abs() < 8.0, "{w}");
}

#[test]
fn rotate_v_spins_faster_than_a_quarter_turn_per_step() {
    // Observed: ParamB 50 turns a small part at 1,550 rad/s.
    let mut dom = Dom::new();
    let (_, wheel) = surface_hinge(&mut dom, MOTOR, CONSTANT, 0.0, 50.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.25);
    let w = vector(&dom, wheel, "AssemblyAngularVelocity").y;
    assert!((w - 1550.0).abs() < 10.0, "{w}");
}

#[test]
fn rotate_v_sin_input() {
    // ω = 31 · A · sin(B · t): A 2, B 3 peaks at 62 near 0.52 s.
    let mut dom = Dom::new();
    let (_, wheel) = surface_hinge(&mut dom, MOTOR, SIN, 2.0, 3.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.5);
    let w = vector(&dom, wheel, "AssemblyAngularVelocity").y;
    assert!((w - 62.0 * (3.0f32 * 0.5).sin()).abs() < 3.0, "{w}");
}

#[test]
fn rotate_p_steps_to_its_angle() {
    let mut dom = Dom::new();
    let (base, wheel) = surface_hinge(&mut dom, STEPPING_MOTOR, CONSTANT, 0.0, 1.0);
    let mut physics = Physics::new();
    // Observed: 0.01 rad per 240 Hz step, 0.2 rad after 5 frames.
    run(&mut physics, &mut dom, 5.0 / 60.0);
    let a = yaw(&dom, base, wheel);
    assert!((a - 0.2).abs() < 0.03, "{a}");
    run(&mut physics, &mut dom, 1.0);
    let a = yaw(&dom, base, wheel);
    assert!((a - 1.0).abs() < 0.01, "{a}");

    // Live changes apply.
    set(&mut dom, base, "TopParamB", Value::Float32(-0.5));
    run(&mut physics, &mut dom, 1.0);
    let a = yaw(&dom, base, wheel);
    assert!((a + 0.5).abs() < 0.01, "{a}");
}

#[test]
fn motors_without_input_hold_still() {
    for surface in [MOTOR, STEPPING_MOTOR] {
        let mut dom = Dom::new();
        let (base, wheel) = surface_hinge(&mut dom, surface, NO_INPUT, 3.0, 2.0);
        let mut physics = Physics::new();
        run(&mut physics, &mut dom, 0.1);
        set(
            &mut dom,
            wheel,
            "AssemblyAngularVelocity",
            v3(0.0, 1.0, 0.0),
        );
        run(&mut physics, &mut dom, 0.1);
        let a = yaw(&dom, base, wheel);
        assert!(a.abs() < 0.02, "surface {surface}: {a}");
    }
}

/// A HingeConstraint between an anchored base and a cube above it, turning
/// about world Y (the attachments' X axes point up).
fn hinge(dom: &mut Dom) -> (InstanceId, InstanceId, InstanceId) {
    let base = part(
        dom,
        "Part",
        Vec3::new(0.0, 1000.0, 0.0),
        Vec3::new(4.0, 1.0, 4.0),
        true,
    );
    let wheel = part(
        dom,
        "Part",
        Vec3::new(0.0, 1003.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let up = CFrame::angles(0.0, 0.0, std::f32::consts::FRAC_PI_2);
    let a0 = child(
        dom,
        "Attachment",
        base,
        &[(
            "CFrame",
            Value::CFrame(CFrame::from_position(Vec3::new(0.0, 1.5, 0.0)) * up),
        )],
    );
    let a1 = child(
        dom,
        "Attachment",
        wheel,
        &[(
            "CFrame",
            Value::CFrame(CFrame::from_position(Vec3::new(0.0, -1.5, 0.0)) * up),
        )],
    );
    let h = child(
        dom,
        "HingeConstraint",
        base,
        &[
            ("Attachment0", Value::Ref(Some(a0.to_ref()))),
            ("Attachment1", Value::Ref(Some(a1.to_ref()))),
        ],
    );
    (base, wheel, h)
}

#[test]
fn hinge_constraint_holds_its_attachments_together() {
    let mut dom = Dom::new();
    let (_, wheel, h) = hinge(&mut dom);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.5);
    assert!((position(&dom, wheel) - Vec3::new(0.0, 1003.0, 0.0)).length() < 0.02);
    assert!(float(&dom, h, "CurrentAngle").abs() < 0.5);
}

#[test]
fn hinge_constraint_motor() {
    let mut dom = Dom::new();
    let (base, wheel, h) = hinge(&mut dom);
    set(&mut dom, h, "ActuatorType", Value::Enum(1));
    set(&mut dom, h, "AngularVelocity", Value::Float32(5.0));
    set(&mut dom, h, "MotorMaxTorque", Value::Float32(1e9));
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.2);
    // Observed: 0.999 rad and 57.2° after 0.2 s.
    let a = yaw(&dom, base, wheel);
    assert!((a - 1.0).abs() < 0.03, "{a}");
    assert!((float(&dom, h, "CurrentAngle") - 57.2).abs() < 2.0);

    // A weak motor speeds up by MotorMaxTorque / inertia (3.73).
    set(&mut dom, h, "MotorMaxTorque", Value::Float32(3.7333));
    set(&mut dom, h, "AngularVelocity", Value::Float32(100.0));
    let before = vector(&dom, wheel, "AssemblyAngularVelocity").y;
    run(&mut physics, &mut dom, 0.5);
    let after = vector(&dom, wheel, "AssemblyAngularVelocity").y;
    assert!((after - before - 0.5).abs() < 0.05, "{before} {after}");
}

#[test]
fn hinge_constraint_servo_and_limits() {
    let mut dom = Dom::new();
    let (base, wheel, h) = hinge(&mut dom);
    set(&mut dom, h, "ActuatorType", Value::Enum(2));
    set(&mut dom, h, "TargetAngle", Value::Float32(90.0));
    set(&mut dom, h, "AngularSpeed", Value::Float32(2.0));
    set(&mut dom, h, "ServoMaxTorque", Value::Float32(1e9));
    let mut physics = Physics::new();
    // Observed: 2 rad/s, 0.416 rad after 0.2 s, at 90° within 1 s.
    run(&mut physics, &mut dom, 0.2);
    let a = yaw(&dom, base, wheel);
    assert!((a - 0.4).abs() < 0.03, "{a}");
    run(&mut physics, &mut dom, 0.8);
    assert!((float(&dom, h, "CurrentAngle") - 90.0).abs() < 1.0);

    set(&mut dom, h, "LimitsEnabled", Value::Bool(true));
    set(&mut dom, h, "UpperAngle", Value::Float32(30.0));
    set(&mut dom, h, "LowerAngle", Value::Float32(-30.0));
    run(&mut physics, &mut dom, 0.5);
    let a = float(&dom, h, "CurrentAngle");
    assert!((a - 30.0).abs() < 2.0, "{a}");
}

/// A 2-stud cube hanging from an anchored base by `class`.
fn hanging(dom: &mut Dom, class: &str, props: &[(&str, Value)]) -> InstanceId {
    let base = part(
        dom,
        "Part",
        Vec3::new(0.0, 1000.0, 0.0),
        Vec3::new(4.0, 1.0, 4.0),
        true,
    );
    let w = part(
        dom,
        "Part",
        Vec3::new(0.0, 995.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let a0 = child(
        dom,
        "Attachment",
        base,
        &[(
            "CFrame",
            Value::CFrame(CFrame::from_position(Vec3::new(0.0, -0.5, 0.0))),
        )],
    );
    let a1 = child(
        dom,
        "Attachment",
        w,
        &[(
            "CFrame",
            Value::CFrame(CFrame::from_position(Vec3::new(0.0, 1.0, 0.0))),
        )],
    );
    let mut all = vec![
        ("Attachment0", Value::Ref(Some(a0.to_ref()))),
        ("Attachment1", Value::Ref(Some(a1.to_ref()))),
    ];
    all.extend(props.iter().cloned());
    child(dom, class, base, &all);
    w
}

/// A constraint class, its properties, and where the cube rests.
type Case = (&'static str, Vec<(&'static str, Value)>, f32);

#[test]
fn ropes_rods_and_springs_settle_where_studio_does() {
    // Observed resting heights below the base's centre: a spring holding
    // its weight (5.6 × 196.2) with Stiffness 1098.72 stretches by 1.
    let cases: [Case; 4] = [
        (
            "SpringConstraint",
            vec![
                ("Stiffness", Value::Float32(1098.72)),
                ("Damping", Value::Float32(50.0)),
                ("FreeLength", Value::Float32(2.0)),
            ],
            -4.5,
        ),
        (
            "RopeConstraint",
            vec![("Length", Value::Float32(6.0))],
            -7.5,
        ),
        ("RodConstraint", vec![("Length", Value::Float32(2.0))], -3.5),
        ("BallSocketConstraint", vec![], -1.5),
    ];
    for (class, props, y) in cases {
        let mut dom = Dom::new();
        let w = hanging(&mut dom, class, &props);
        let mut physics = Physics::new();
        run(&mut physics, &mut dom, 4.0);
        let at = position(&dom, w).y - 1000.0;
        assert!((at - y).abs() < 0.1, "{class}: {at}");
    }
}

#[test]
fn disabling_a_constraint_frees_its_parts() {
    let mut dom = Dom::new();
    let w = hanging(
        &mut dom,
        "RopeConstraint",
        &[("Length", Value::Float32(6.0))],
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 1.0);
    let rope = dom
        .descendants(dom.find_service("Workspace").unwrap())
        .into_iter()
        .find(|i| dom.class_name(*i) == "RopeConstraint")
        .unwrap();
    set(&mut dom, rope, "Enabled", Value::Bool(false));
    run(&mut physics, &mut dom, 0.5);
    assert!(position(&dom, w).y < 980.0);
}
