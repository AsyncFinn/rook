//! Joints, assemblies and MakeJoints. Expected values were observed in
//! Studio 0.741.

use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, Value};

use super::{block, part, position, run, set};
use crate::{Physics, assembly, joints};

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
    let c0 = dom.cframe_property(a, "CFrame").unwrap().inverse()
        * dom.cframe_property(b, "CFrame").unwrap();
    joint(dom, "Weld", a, b, c0)
}

pub(super) fn at(x: f32, y: f32, z: f32) -> CFrame {
    CFrame::from_position(Vec3::new(x, y, z))
}

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 1e-3
}

#[test]
fn welded_parts_fall_as_one_body() {
    let mut dom = Dom::new();
    let a = block(&mut dom, Vec3::new(0.0, 20.0, 0.0), false);
    let b = block(&mut dom, Vec3::new(2.0, 20.0, 0.0), false);
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
    let base = block(&mut dom, Vec3::new(0.0, 20.0, 0.0), true);
    let hanging = block(&mut dom, Vec3::new(4.0, 20.0, 0.0), false);
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
    let small = block(&mut dom, Vec3::new(0.0, 50.0, 0.0), false);
    let big = part(
        &mut dom,
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
    let a = block(&mut dom, Vec3::new(0.0, 20.0, 0.0), true);
    let b = block(&mut dom, Vec3::new(4.0, 20.0, 0.0), false);
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
    let a = part(&mut dom, Vec3::new(0.0, 50.0, 0.0), Vec3::splat(3.0), true);
    let b = block(&mut dom, Vec3::new(4.0, 50.0, 0.0), false);
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
    let rel = dom.cframe_property(a, "CFrame").unwrap().inverse()
        * dom.cframe_property(b, "CFrame").unwrap();
    let expected = Vec3::new(4.0 + 1f32.sin(), -1f32.cos(), 0.0);
    assert!(close(rel.position, expected), "{rel:?}");

    // Transform applies too.
    set(&mut dom, m, "DesiredAngle", Value::Float32(0.0));
    set(&mut dom, m, "CurrentAngle", Value::Float32(0.0));
    set(&mut dom, m, "Transform", Value::CFrame(at(0.0, 0.0, 2.0)));
    run(&mut physics, &mut dom, 1.0 / 60.0);
    let rel = dom.cframe_property(a, "CFrame").unwrap().inverse()
        * dom.cframe_property(b, "CFrame").unwrap();
    assert!(close(rel.position, Vec3::new(4.0, -1.0, 2.0)), "{rel:?}");
}

#[test]
fn a_hinged_bar_swings_about_its_pivot() {
    // A 4-stud bar hinged at its end to an anchored block, axis along Z,
    // swings down under gravity and stays on the pivot.
    let mut dom = Dom::new();
    let block = block(&mut dom, Vec3::new(0.0, 50.0, 0.0), true);
    let bar = part(
        &mut dom,
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
    let bar_cf = dom.cframe_property(bar, "CFrame").unwrap();
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
    let a = block(&mut dom, Vec3::new(0.0, 20.0, 0.0), true);
    let b = block(&mut dom, Vec3::new(5.0, 23.0, 0.0), false);
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
    let a = block(&mut dom, Vec3::ZERO, true);
    let b = block(&mut dom, Vec3::new(4.0, 0.0, 0.0), false);
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
    let a = block(&mut dom, Vec3::ZERO, false);
    let b = part(&mut dom, Vec3::new(3.0, 0.0, 0.0), Vec3::splat(4.0), false);
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
    let a = block(&mut dom, Vec3::ZERO, true);
    let b = block(&mut dom, Vec3::new(2.0, 0.0, 0.0), true);
    let c = block(&mut dom, Vec3::new(4.0, 0.0, 0.0), false);
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
    let a = block(&mut dom, Vec3::ZERO, true);
    let c = block(&mut dom, Vec3::new(2.0, 0.0, 0.0), false);
    let b = block(&mut dom, Vec3::new(4.0, 0.0, 0.0), true);
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
    let a = block(&mut dom, Vec3::new(0.0, 20.0, 0.0), false);
    let b = block(&mut dom, Vec3::new(2.0, 20.0, 0.0), false);
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
