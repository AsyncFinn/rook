//! Joints, assemblies and MakeJoints. Expected values were observed in
//! Studio 0.741.

use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, Value};

use super::{cframe, part, position, run, set};
use crate::{Physics, assembly, joints, surfaces};

fn joint(
    dom: &mut Dom,
    class: &str,
    part0: InstanceId,
    part1: InstanceId,
    c0: CFrame,
) -> InstanceId {
    let j = dom.create_by_name(class).unwrap();
    set(dom, j, "Part0", Value::Ref(Some(part0.to_ref())));
    set(dom, j, "Part1", Value::Ref(Some(part1.to_ref())));
    if class != "WeldConstraint" {
        set(dom, j, "C0", Value::CFrame(c0));
    }
    dom.set_parent(j, Some(part0)).unwrap();
    j
}

/// A weld holding `b` where it is relative to `a`.
fn weld(dom: &mut Dom, a: InstanceId, b: InstanceId) -> InstanceId {
    let c0 = cframe(dom, a).inverse() * cframe(dom, b);
    joint(dom, "Weld", a, b, c0)
}

fn at(x: f32, y: f32, z: f32) -> CFrame {
    CFrame::from_position(Vec3::new(x, y, z))
}

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 1e-3
}

#[test]
fn welded_parts_fall_as_one_body() {
    let mut dom = Dom::new();
    let a = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 20.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let b = part(
        &mut dom,
        "Part",
        Vec3::new(2.0, 20.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    weld(&mut dom, a, b);
    let mut physics = Physics::new();
    physics.sync(&mut dom);
    assert_eq!(physics.part_count(), 2);
    assert_eq!(physics.body_count(), 1);
    run(&mut physics, &mut dom, 0.25);
    let (pa, pb) = (position(&dom, a), position(&dom, b));
    assert!(pa.y < 15.0);
    assert!(close(pb - pa, Vec3::new(2.0, 0.0, 0.0)), "{pa} {pb}");
}

#[test]
fn a_weld_to_an_anchored_part_holds_the_assembly() {
    let mut dom = Dom::new();
    let base = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 20.0, 0.0),
        Vec3::splat(2.0),
        true,
    );
    let hanging = part(
        &mut dom,
        "Part",
        Vec3::new(4.0, 20.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let w = weld(&mut dom, base, hanging);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.5);
    assert_eq!(physics.body_count(), 0);
    assert_eq!(position(&dom, hanging), Vec3::new(4.0, 20.0, 0.0));

    // Breaking the weld lets it fall.
    dom.destroy(w).unwrap();
    run(&mut physics, &mut dom, 0.5);
    assert!(position(&dom, hanging).y < 0.0);
}

#[test]
fn an_active_joint_snaps_the_non_root_part() {
    // The larger part is the root and stays; the other moves to satisfy C0.
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    set(&mut dom, ws, "Gravity", Value::Float32(0.0));
    let small = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 50.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let big = part(
        &mut dom,
        "Part",
        Vec3::new(10.0, 50.0, 0.0),
        Vec3::splat(3.0),
        false,
    );
    joint(&mut dom, "Weld", small, big, at(0.0, 5.0, 0.0));
    let mut physics = Physics::new();
    physics.sync(&mut dom);
    assert_eq!(physics.root_of(small), Some(big));
    assert_eq!(position(&dom, big), Vec3::new(10.0, 50.0, 0.0));
    assert!(close(position(&dom, small), Vec3::new(10.0, 45.0, 0.0)));
}

#[test]
fn joints_outside_workspace_or_disabled_are_inactive() {
    let mut dom = Dom::new();
    let a = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 20.0, 0.0),
        Vec3::splat(2.0),
        true,
    );
    let b = part(
        &mut dom,
        "Part",
        Vec3::new(4.0, 20.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let w = weld(&mut dom, a, b);
    let storage = dom.get_service("ServerStorage").unwrap();
    dom.set_parent(w, Some(storage)).unwrap();
    assert!(joints::active_joint(&dom, w).is_none());
    // JointsService counts as being in the world.
    let js = dom.get_service("JointsService").unwrap();
    dom.set_parent(w, Some(js)).unwrap();
    assert!(joints::active_joint(&dom, w).is_some());
    set(&mut dom, w, "Enabled", Value::Bool(false));
    assert!(joints::active_joint(&dom, w).is_none());
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.5);
    assert!(position(&dom, b).y < 20.0);
}

#[test]
fn motors_turn_toward_the_desired_angle() {
    // CurrentAngle moves 0.1 per frame with MaxVelocity 0.1, and
    // A⁻¹·B = C0 · Rz(angle) · C1⁻¹.
    let mut dom = Dom::new();
    let a = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 50.0, 0.0),
        Vec3::splat(3.0),
        true,
    );
    let b = part(
        &mut dom,
        "Part",
        Vec3::new(4.0, 50.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let m = joint(&mut dom, "Motor6D", a, b, at(4.0, 0.0, 0.0));
    set(&mut dom, m, "C1", Value::CFrame(at(0.0, 1.0, 0.0)));
    set(&mut dom, m, "MaxVelocity", Value::Float32(0.1));
    set(&mut dom, m, "DesiredAngle", Value::Float32(1.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 1.0 / 60.0);
    let angle = |dom: &Dom| match dom.property_by_name(m, "CurrentAngle") {
        Some(Value::Float32(a)) => a,
        other => panic!("{other:?}"),
    };
    assert!((angle(&dom) - 0.1).abs() < 1e-6);
    run(&mut physics, &mut dom, 0.5);
    assert_eq!(angle(&dom), 1.0);
    let rel = cframe(&dom, a).inverse() * cframe(&dom, b);
    let expected = Vec3::new(4.0 + 1f32.sin(), -1f32.cos(), 0.0);
    assert!(close(rel.position, expected), "{rel:?}");

    // Transform applies too.
    set(&mut dom, m, "DesiredAngle", Value::Float32(0.0));
    set(&mut dom, m, "CurrentAngle", Value::Float32(0.0));
    set(&mut dom, m, "Transform", Value::CFrame(at(0.0, 0.0, 2.0)));
    run(&mut physics, &mut dom, 1.0 / 60.0);
    let rel = cframe(&dom, a).inverse() * cframe(&dom, b);
    assert!(close(rel.position, Vec3::new(4.0, -1.0, 2.0)), "{rel:?}");
}

#[test]
fn a_hinged_bar_swings_about_its_pivot() {
    // A 4-stud bar hinged at its end to an anchored block, axis along Z,
    // swings down under gravity and stays on the pivot.
    let mut dom = Dom::new();
    let block = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 50.0, 0.0),
        Vec3::splat(2.0),
        true,
    );
    let bar = part(
        &mut dom,
        "Part",
        Vec3::new(3.0, 50.0, 0.0),
        Vec3::new(4.0, 1.0, 1.0),
        false,
    );
    let hinge = joint(&mut dom, "Rotate", block, bar, at(1.0, 0.0, 0.0));
    set(&mut dom, hinge, "C1", Value::CFrame(at(-2.0, 0.0, 0.0)));
    let mut physics = Physics::new();
    physics.sync(&mut dom);
    assert_eq!(physics.body_count(), 1);
    run(&mut physics, &mut dom, 1.5);
    let bar_cf = cframe(&dom, bar);
    let end = bar_cf * rook_types::Vector3::new(-2.0, 0.0, 0.0);
    assert!(
        (end.0 - Vec3::new(1.0, 50.0, 0.0)).length() < 0.05,
        "{end:?}"
    );
    assert!(bar_cf.position.y < 49.0, "{bar_cf:?}");
}

#[test]
fn weld_constraints_keep_the_pose_they_started_with() {
    let mut dom = Dom::new();
    let a = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 20.0, 0.0),
        Vec3::splat(2.0),
        true,
    );
    let b = part(
        &mut dom,
        "Part",
        Vec3::new(5.0, 23.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    joint(&mut dom, "WeldConstraint", a, b, CFrame::IDENTITY);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.5);
    assert_eq!(position(&dom, b), Vec3::new(5.0, 23.0, 0.0));
}

#[test]
fn moving_a_part_moves_its_assembly_and_reseating_rewrites_its_side() {
    // Setting CFrame moves the whole assembly, anchored root included;
    // setting Position moves only the part and rewrites its C0 or C1.
    let mut dom = Dom::new();
    let a = part(&mut dom, "Part", Vec3::ZERO, Vec3::splat(2.0), true);
    let b = part(
        &mut dom,
        "Part",
        Vec3::new(4.0, 0.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let w = joint(&mut dom, "Weld", a, b, at(2.0, 0.0, 0.0));
    set(&mut dom, w, "C1", Value::CFrame(at(-2.0, 0.0, 0.0)));
    assembly::move_part(&mut dom, b, at(4.0, 10.0, 0.0)).unwrap();
    assert_eq!(position(&dom, a), Vec3::new(0.0, 10.0, 0.0));

    set(&mut dom, b, "CFrame", Value::CFrame(at(4.0, 20.0, 0.0)));
    joints::reseat(&mut dom, b).unwrap();
    let c = |dom: &Dom, name| dom.property_by_name(w, name);
    assert_eq!(c(&dom, "C1"), Some(Value::CFrame(at(-2.0, -10.0, 0.0))));
    set(&mut dom, a, "CFrame", Value::CFrame(at(0.0, 0.0, 0.0)));
    joints::reseat(&mut dom, a).unwrap();
    assert_eq!(c(&dom, "C0"), Some(Value::CFrame(at(2.0, 10.0, 0.0))));
}

#[test]
fn root_part_rules() {
    let mut dom = Dom::new();
    let a = part(&mut dom, "Part", Vec3::ZERO, Vec3::splat(2.0), false);
    let b = part(
        &mut dom,
        "Part",
        Vec3::new(3.0, 0.0, 0.0),
        Vec3::splat(4.0),
        false,
    );
    weld(&mut dom, a, b);
    let root = |dom: &Dom| assembly::Assembly::of(dom, a).unwrap().root();
    assert_eq!(root(&dom), b, "larger part");
    set(&mut dom, a, "RootPriority", Value::Int32(1));
    assert_eq!(root(&dom), a, "higher RootPriority");
    set(&mut dom, a, "RootPriority", Value::Int32(0));
    set(&mut dom, b, "Massless", Value::Bool(true));
    assert_eq!(root(&dom), a, "non-Massless");
    let asm = assembly::Assembly::of(&dom, a).unwrap();
    assert!((asm.mass(&dom) - 8.0 * 0.7).abs() < 1e-4);
    set(&mut dom, b, "Anchored", Value::Bool(true));
    let asm = assembly::Assembly::of(&dom, a).unwrap();
    assert_eq!(asm.root(), b, "anchored");
    assert_eq!(asm.mass(&dom), f32::INFINITY);
}

#[test]
fn anchored_parts_root_their_own_assemblies() {
    // A and B anchored, C unanchored, welded A-B and B-C: A is alone, C
    // joins B, and moving B takes C but not A. GetConnectedParts still
    // reaches all three.
    let mut dom = Dom::new();
    let a = part(&mut dom, "Part", Vec3::ZERO, Vec3::splat(2.0), true);
    let b = part(
        &mut dom,
        "Part",
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::splat(2.0),
        true,
    );
    let c = part(
        &mut dom,
        "Part",
        Vec3::new(4.0, 0.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    weld(&mut dom, a, b);
    weld(&mut dom, b, c);
    let root = |dom: &Dom, p| assembly::Assembly::of(dom, p).unwrap().root();
    assert_eq!((root(&dom, a), root(&dom, b), root(&dom, c)), (a, b, b));
    assert_eq!(assembly::connected_parts(&dom, a, true).len(), 3);
    assert_eq!(assembly::connected_parts(&dom, a, false).len(), 2);
    assembly::move_part(&mut dom, b, at(2.0, 10.0, 0.0)).unwrap();
    assert_eq!(position(&dom, a), Vec3::ZERO);
    assert_eq!(position(&dom, c), Vec3::new(4.0, 10.0, 0.0));

    // Unanchoring B joins it, and C, to A's assembly.
    set(&mut dom, b, "Anchored", Value::Bool(false));
    assert_eq!((root(&dom, b), root(&dom, c)), (a, a));
}

#[test]
fn a_cluster_between_anchored_parts_joins_the_newest_joint() {
    let mut dom = Dom::new();
    let a = part(&mut dom, "Part", Vec3::ZERO, Vec3::splat(2.0), true);
    let c = part(
        &mut dom,
        "Part",
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let b = part(
        &mut dom,
        "Part",
        Vec3::new(4.0, 0.0, 0.0),
        Vec3::splat(2.0),
        true,
    );
    weld(&mut dom, a, c);
    weld(&mut dom, c, b);
    let root = |dom: &Dom, p| assembly::Assembly::of(dom, p).unwrap().root();
    assert_eq!(root(&dom, c), b);
    assert_eq!(root(&dom, b), b);
    assert_eq!(root(&dom, a), a);
    let asm = assembly::Assembly::of(&dom, a).unwrap();
    assert_eq!(asm.members.len(), 1);
}

#[test]
fn anchoring_a_part_splits_its_simulated_assembly() {
    let mut dom = Dom::new();
    let a = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 20.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let b = part(
        &mut dom,
        "Part",
        Vec3::new(2.0, 20.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    weld(&mut dom, a, b);
    let mut physics = Physics::new();
    physics.sync(&mut dom);
    assert_eq!(physics.body_count(), 1);
    set(&mut dom, a, "Anchored", Value::Bool(true));
    run(&mut physics, &mut dom, 0.5);
    assert_eq!(physics.body_count(), 0);
    assert_eq!(physics.root_of(b), Some(a));
    assert_eq!(position(&dom, b), Vec3::new(2.0, 20.0, 0.0));
}

// ---- MakeJoints -------------------------------------------------------------

fn surfaces(dom: &mut Dom, id: InstanceId, surface: &str) {
    let e = dom.database().enum_by_name("SurfaceType").unwrap();
    let n = e.items.iter().find(|(name, _)| *name == surface).unwrap().1;
    for face in ["Top", "Bottom", "Left", "Right", "Front", "Back"] {
        set(dom, id, &format!("{face}Surface"), Value::Enum(n));
    }
}

/// Two 4×1×2 parts stacked `gap` apart; the upper one calls MakeJoints.
/// Returns the joints' classes and whether the upper part is Part0.
fn stacked(lower: &str, upper: &str, gap: f32) -> Vec<(String, bool)> {
    let mut dom = Dom::new();
    let size = Vec3::new(4.0, 1.0, 2.0);
    let bottom = part(&mut dom, "Part", Vec3::ZERO, size, true);
    let top = part(&mut dom, "Part", Vec3::new(0.0, 1.0 + gap, 0.0), size, true);
    surfaces(&mut dom, bottom, lower);
    surfaces(&mut dom, top, upper);
    surfaces::make_joints(&mut dom, &[top]).unwrap();
    joints::part_joints(&dom, top)
        .iter()
        .map(|j| (dom.class_name(j.id).to_owned(), j.part0 == top))
        .collect()
}

#[test]
fn surface_types_pick_the_joint() {
    let one = |lower, upper| stacked(lower, upper, 0.0);
    let joint = |class: &str, upper_is_part0| vec![(class.to_owned(), upper_is_part0)];
    assert_eq!(one("Studs", "Inlet"), joint("Snap", true));
    assert_eq!(one("Universal", "Universal"), joint("Snap", true));
    assert_eq!(one("Studs", "Studs"), vec![]);
    assert_eq!(one("Inlet", "Inlet"), vec![]);
    assert_eq!(one("Smooth", "Smooth"), vec![]);
    assert_eq!(one("Smooth", "Weld"), joint("Weld", true));
    assert_eq!(one("Glue", "Studs"), joint("Glue", true));
    assert_eq!(one("Glue", "Weld"), joint("Weld", true));
    assert_eq!(one("Hinge", "Weld"), joint("Rotate", false));
    assert_eq!(one("Motor", "Smooth"), joint("RotateV", false));
    assert_eq!(one("SteppingMotor", "Studs"), joint("RotateP", false));
    assert_eq!(one("Motor", "Hinge"), joint("Rotate", true));
}

#[test]
fn faces_join_only_when_close_and_aligned() {
    assert_eq!(stacked("Weld", "Weld", 0.0099).len(), 1);
    assert_eq!(stacked("Weld", "Weld", 0.011).len(), 0);
    assert_eq!(stacked("Weld", "Weld", -0.008).len(), 1);
    assert_eq!(stacked("Weld", "Weld", -0.011).len(), 0);

    let yawed = |degrees: f32| {
        let mut dom = Dom::new();
        let a = part(&mut dom, "Part", Vec3::ZERO, Vec3::splat(2.0), true);
        let b = part(&mut dom, "Part", Vec3::ZERO, Vec3::splat(2.0), true);
        let cf = at(0.0, 2.0, 0.0) * CFrame::angles(0.0, degrees.to_radians(), 0.0);
        set(&mut dom, b, "CFrame", Value::CFrame(cf));
        surfaces(&mut dom, a, "Weld");
        surfaces::make_joints(&mut dom, &[a]).unwrap();
        joints::part_joints(&dom, a).len()
    };
    assert_eq!(yawed(0.5), 1);
    assert_eq!(yawed(0.6), 0);
    assert_eq!(yawed(30.0), 0);
    assert_eq!(yawed(90.0), 1);

    // Touching along an edge isn't enough.
    let mut dom = Dom::new();
    let size = Vec3::new(4.0, 1.0, 2.0);
    let a = part(&mut dom, "Part", Vec3::ZERO, size, true);
    part(&mut dom, "Part", Vec3::new(4.0, 1.0, 0.0), size, true);
    surfaces(&mut dom, a, "Weld");
    surfaces::make_joints(&mut dom, &[a]).unwrap();
    assert!(joints::part_joints(&dom, a).is_empty());
}

#[test]
fn make_joints_frames_match_studio() {
    let check = |dir: Vec3, c0: [f32; 12], c1: [f32; 12]| {
        let mut dom = Dom::new();
        let a = part(&mut dom, "Part", Vec3::ZERO, Vec3::splat(2.0), true);
        part(&mut dom, "Part", dir * 2.0, Vec3::splat(2.0), true);
        surfaces(&mut dom, a, "Weld");
        surfaces::make_joints(&mut dom, &[a]).unwrap();
        let j = joints::part_joints(&dom, a)[0];
        assert_eq!(j.part0, a);
        assert_eq!(dom.parent(j.id), Some(a));
        assert_eq!(dom.name(j.id), "Weld");
        let get = |name| match dom.property_by_name(j.id, name) {
            Some(Value::CFrame(c)) => c,
            other => panic!("{other:?}"),
        };
        let (got0, got1) = (get("C0"), get("C1"));
        assert!(
            got0.fuzzy_eq(&CFrame::from_components(c0), 1e-5),
            "{dir} C0 {got0:?}"
        );
        assert!(
            got1.fuzzy_eq(&CFrame::from_components(c1), 1e-5),
            "{dir} C1 {got1:?}"
        );
    };
    // Recorded with :MakeJoints() on 2×2×2 parts, as CFrame components.
    #[rustfmt::skip]
    let cases = [
        (Vec3::Y, [0., 1., 0., -1., 0., 0., 0., 0., 1., 0., 1., 0.], [0., -1., 0., -1., 0., 0., 0., 0., 1., 0., 1., 0.]),
        (-Vec3::Y, [0., -1., 0., 1., 0., 0., 0., 0., -1., 0., 1., 0.], [0., 1., 0., 1., 0., 0., 0., 0., -1., 0., 1., 0.]),
        (Vec3::X, [1., 0., 0., 0., 0., 1., 0., 1., 0., -1., 0., 0.], [-1., 0., 0., 0., 0., 1., 0., 1., 0., -1., 0., 0.]),
        (-Vec3::X, [-1., 0., 0., 0., 0., -1., 0., 1., 0., 1., 0., 0.], [1., 0., 0., 0., 0., -1., 0., 1., 0., 1., 0., 0.]),
        (Vec3::Z, [0., 0., 1., 1., 0., 0., 0., 1., 0., 0., 0., 1.], [0., 0., -1., 1., 0., 0., 0., 1., 0., 0., 0., 1.]),
        (-Vec3::Z, [0., 0., -1., -1., 0., 0., 0., 1., 0., 0., 0., -1.], [0., 0., 1., -1., 0., 0., 0., 1., 0., 0., 0., -1.]),
    ];
    for (dir, c0, c1) in cases {
        check(dir, c0, c1);
    }
}

#[test]
fn model_parts_join_parts_outside_and_only_once() {
    // Model:MakeJoints on A and B joins A-B and A-C (C is outside the
    // model); B doesn't join A again, and a second call adds nothing.
    let mut dom = Dom::new();
    let a = part(&mut dom, "Part", Vec3::ZERO, Vec3::splat(2.0), true);
    let b = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::splat(2.0),
        true,
    );
    let c = part(
        &mut dom,
        "Part",
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::splat(2.0),
        true,
    );
    for p in [a, b, c] {
        surfaces(&mut dom, p, "Weld");
    }
    surfaces::make_joints(&mut dom, &[a, b]).unwrap();
    surfaces::make_joints(&mut dom, &[a, b]).unwrap();
    assert_eq!(joints::part_joints(&dom, a).len(), 2);
    assert_eq!(joints::part_joints(&dom, b).len(), 1);
    assert_eq!(joints::part_joints(&dom, c).len(), 1);
}

#[test]
fn many_callers_sweep_the_world_with_the_same_result() {
    // A column of 20 parts: each neighbouring pair joins once, Part0 being
    // the one earlier in the list.
    let mut dom = Dom::new();
    let column: Vec<_> = (0..20)
        .map(|i| {
            part(
                &mut dom,
                "Part",
                Vec3::new(0.0, 2.0 * i as f32, 0.0),
                Vec3::splat(2.0),
                true,
            )
        })
        .collect();
    for &p in &column {
        surfaces(&mut dom, p, "Weld");
    }
    let mut callers = column.clone();
    callers.reverse();
    surfaces::make_joints(&mut dom, &callers).unwrap();
    for pair in column.windows(2) {
        let js = joints::part_joints(&dom, pair[0]);
        let j = js.iter().find(|j| j.other(pair[0]) == pair[1]).unwrap();
        assert_eq!(j.part0, pair[1]);
    }
    let total: usize = column
        .iter()
        .map(|p| joints::part_joints(&dom, *p).len())
        .sum();
    assert_eq!(total, 2 * 19);
}

#[test]
fn wedge_slopes_join_like_studio() {
    // A 2×4×4 wedge with a 2×1×√32 slab lying on its slope, everything
    // Smooth except `on_wedge` and the slab's bottom. Returns the joint's
    // class, whether the wedge is Part0, and C0's position.
    let trial = |on_wedge: &[(&str, &str)],
                 slab_bottom: &str,
                 wedge_calls: bool|
     -> Option<(String, bool, Vec3)> {
        let mut dom = Dom::new();
        let size = Vec3::new(2.0, 4.0, 4.0);
        let wedge = part(&mut dom, "WedgePart", Vec3::ZERO, size, true);
        let slab_size = Vec3::new(2.0, 1.0, 32f32.sqrt());
        let slab = part(&mut dom, "Part", Vec3::ZERO, slab_size, true);
        let on_slope = Vec3::new(0.0, 1.0, -1.0).normalize() * 0.5;
        let cf =
            at(on_slope.x, on_slope.y, on_slope.z) * CFrame::angles(-45f32.to_radians(), 0.0, 0.0);
        set(&mut dom, slab, "CFrame", Value::CFrame(cf));
        surfaces(&mut dom, wedge, "Smooth");
        surfaces(&mut dom, slab, "Smooth");
        let e = dom.database().enum_by_name("SurfaceType").unwrap();
        let n = |s: &str| Value::Enum(e.items.iter().find(|(name, _)| *name == s).unwrap().1);
        let wedge_surfaces: Vec<_> = on_wedge.iter().map(|&(f, s)| (f, n(s))).collect();
        let bottom = n(slab_bottom);
        for (face, v) in wedge_surfaces {
            set(&mut dom, wedge, &format!("{face}Surface"), v);
        }
        set(&mut dom, slab, "BottomSurface", bottom);
        let caller = if wedge_calls { wedge } else { slab };
        surfaces::make_joints(&mut dom, &[caller]).unwrap();
        let js = joints::part_joints(&dom, wedge);
        assert!(js.len() <= 1);
        let j = js.first()?;
        let Some(Value::CFrame(c0)) = dom.property_by_name(j.id, "C0") else {
            panic!("no C0")
        };
        Some((
            dom.class_name(j.id).to_owned(),
            j.part0 == wedge,
            c0.position,
        ))
    };
    let joined = |class: &str, wedge_is_part0, c0: [f32; 3]| {
        Some((class.to_owned(), wedge_is_part0, Vec3::from(c0)))
    };
    let slab_face = [0.0, -0.5, 0.0];

    // The wedge calling: only its own Weld, Top before Front.
    let top = [0.0, 2.0, 0.0];
    let front = [0.0, 0.0, -2.0];
    assert_eq!(
        trial(&[("Top", "Weld")], "Smooth", true),
        joined("Weld", true, top)
    );
    assert_eq!(
        trial(&[("Front", "Weld")], "Smooth", true),
        joined("Weld", true, front)
    );
    let both = [("Top", "Weld"), ("Front", "Weld")];
    assert_eq!(trial(&both, "Smooth", true), joined("Weld", true, top));
    let glue_top = [("Top", "Glue"), ("Front", "Weld")];
    assert_eq!(
        trial(&glue_top, "Smooth", true),
        joined("Weld", true, front)
    );
    assert_eq!(trial(&[], "Weld", true), None);
    assert_eq!(trial(&[("Front", "Universal")], "Universal", true), None);
    assert_eq!(trial(&[("Front", "Hinge")], "Weld", true), None);

    // The slab calling: the slope is the wedge's Front.
    assert_eq!(trial(&[], "Weld", false), joined("Weld", false, slab_face));
    assert_eq!(
        trial(&[("Front", "Weld")], "Smooth", false),
        joined("Weld", false, slab_face)
    );
    assert_eq!(trial(&[("Top", "Weld")], "Smooth", false), None);
    assert_eq!(
        trial(&[("Front", "Studs")], "Inlet", false),
        joined("Snap", false, slab_face)
    );
    assert_eq!(trial(&[("Top", "Studs")], "Inlet", false), None);
    assert_eq!(trial(&[], "Hinge", false), None);

    // The box's top and front aren't faces.
    let mut dom = Dom::new();
    let wedge = part(
        &mut dom,
        "WedgePart",
        Vec3::ZERO,
        Vec3::new(2.0, 4.0, 4.0),
        true,
    );
    part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 2.5, 0.0),
        Vec3::new(2.0, 1.0, 4.0),
        true,
    );
    part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 0.0, -2.5),
        Vec3::new(2.0, 4.0, 1.0),
        true,
    );
    surfaces(&mut dom, wedge, "Weld");
    surfaces::make_joints(&mut dom, &[wedge]).unwrap();
    assert!(joints::part_joints(&dom, wedge).is_empty());
}

#[test]
fn surface_type_numbers_match_the_database() {
    let e = rook_reflection::database()
        .enum_by_name("SurfaceType")
        .unwrap();
    let value = |name: &str| e.items.iter().find(|(n, _)| *n == name).unwrap().1;
    let names = [
        "Glue",
        "Weld",
        "Studs",
        "Inlet",
        "Universal",
        "Hinge",
        "Motor",
        "SteppingMotor",
    ];
    for (i, name) in names.iter().enumerate() {
        assert_eq!(value(name), i as u32 + 1, "{name}");
    }
}
