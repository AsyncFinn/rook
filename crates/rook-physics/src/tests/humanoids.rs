//! The Humanoid controller in a world of its own (Rook's controller; the
//! numbers fitted to Studio are checked in rook-engine's movement tests).

use super::*;
use crate::humanoid::{Drive, State};

/// Where a Humanoid's root walking up a staircase is every 0.05 s for
/// 1 s, stepped in frames `hz` times a second.
fn climb_stairs(hz: u32) -> Vec<Vec3> {
    let mut dom = Dom::new();
    floor(&mut dom);
    for (i, top) in [0.9, 1.3, 1.7].into_iter().enumerate() {
        let z = -4.0 - 2.0 * i as f32;
        part(
            &mut dom,
            Vec3::new(0.0, top - 0.5, z - 5.0),
            Vec3::new(8.0, 1.0, 10.0),
            true,
        );
    }
    let ws = dom.get_service("Workspace").unwrap();
    let model = child(&mut dom, "Model", ws, &[]);
    let humanoid = child(&mut dom, "Humanoid", model, &[]);
    let root = part(
        &mut dom,
        Vec3::new(0.0, 3.5, 2.0),
        Vec3::new(2.0, 2.0, 1.0),
        false,
    );
    dom.set_parent(root, Some(model)).unwrap();
    let drive = Drive {
        humanoid,
        root,
        root_size: Vec3::new(2.0, 2.0, 1.0),
        standing_height: 3.0,
        walk: Vec3::new(0.0, 0.0, -16.0),
        walk_speed: 16.0,
        jump: false,
        jump_speed: 50.0,
        max_slope: 89f32.to_radians(),
        auto_rotate: true,
        face: None,
        platform_stand: false,
        sit: false,
        dead: false,
        disabled: 0,
    };
    let mut physics = Physics::new();
    let mut path = Vec::new();
    let per_sample = hz / 20;
    for frame in 1..=hz {
        physics.sync(&mut dom);
        physics.drive_humanoids(vec![drive.clone()]);
        physics.step(1.0 / f64::from(hz));
        physics.write_back(&mut dom);
        if frame % per_sample == 0 {
            path.push(position(&dom, root));
        }
    }
    path
}

/// The controller feels the floor every 240 Hz step, so a walk up stairs
/// takes the same path at any frame rate. Felt once a frame, the steps of
/// a 20 Hz frame steered by a floor up to 50 ms old.
#[test]
fn a_walk_is_the_same_at_any_frame_rate() {
    let fast = climb_stairs(60);
    let slow = climb_stairs(20);
    assert!(fast.last().unwrap().y > 4.5, "climbed: {:?}", fast.last());
    let worst = fast
        .iter()
        .zip(&slow)
        .map(|(a, b)| (*a - *b).length())
        .fold(0.0, f32::max);
    assert!(worst < 1e-3, "paths differ by up to {worst} studs");
}

/// A Humanoid standing still on the floor, stepped a little to settle it.
fn standing(dom: &mut Dom, physics: &mut Physics) -> Drive {
    floor(dom);
    let ws = dom.get_service("Workspace").unwrap();
    let model = child(dom, "Model", ws, &[]);
    let humanoid = child(dom, "Humanoid", model, &[]);
    let size = Vec3::new(2.0, 2.0, 1.0);
    let root = part(dom, Vec3::new(0.0, 3.5, 0.0), size, false);
    dom.set_parent(root, Some(model)).unwrap();
    let drive = Drive {
        humanoid,
        root,
        root_size: size,
        standing_height: 3.0,
        walk: Vec3::ZERO,
        walk_speed: 16.0,
        jump: false,
        jump_speed: 50.0,
        max_slope: 89f32.to_radians(),
        auto_rotate: true,
        face: None,
        platform_stand: false,
        sit: false,
        dead: false,
        disabled: 0,
    };
    frames(dom, physics, &drive, 6);
    drive
}

/// Run `count` 60 Hz frames, as a DataModel does.
fn frames(dom: &mut Dom, physics: &mut Physics, drive: &Drive, count: u32) {
    for _ in 0..count {
        physics.sync(dom);
        physics.drive_humanoids(vec![drive.clone()]);
        physics.step(1.0 / 60.0);
        physics.write_back(dom);
    }
}

/// Lean `root` over by `degrees`, as a script setting its CFrame does.
fn lean_over(dom: &mut Dom, root: InstanceId, degrees: f32) {
    let at = position(dom, root);
    let mut cf = CFrame::angles(degrees.to_radians(), 0.0, 0.0);
    cf.position = at;
    set(dom, root, "CFrame", Value::CFrame(cf));
}

/// How far `root` leans off vertical, in degrees.
fn lean(dom: &Dom, root: InstanceId) -> f32 {
    let cf = dom.cframe_property(root, "CFrame").unwrap();
    cf.up_vector().0.angle_between(Vec3::Y).to_degrees()
}

/// A Humanoid turning on the spot carrying something heavy off to one
/// side, as a tool or a hat is: Rapier's gyroscopic pass turns a spin about
/// Y into spin about X and Z whatever the body's locks say, so each turn
/// used to leave a little lean that the lock then kept for good.
#[test]
fn turning_with_a_weight_off_to_one_side_leaves_no_lean() {
    let mut dom = Dom::new();
    let mut physics = Physics::new();
    let mut drive = standing(&mut dom, &mut physics);
    let root = drive.root;
    let beside = Vec3::new(2.5, 1.5, 1.0);
    let at = position(&dom, root) + beside;
    let held = part(&mut dom, at, Vec3::ONE, false);
    child(
        &mut dom,
        "Weld",
        root,
        &[
            ("Part0", Value::Ref(Some(root.to_ref()))),
            ("Part1", Value::Ref(Some(held.to_ref()))),
            ("C0", Value::CFrame(CFrame::from_position(beside))),
        ],
    );
    for frame in 0..600 {
        let (x, z) = (frame as f32 * 0.2).sin_cos();
        drive.walk = Vec3::new(x, 0.0, z) * 16.0;
        frames(&mut dom, &mut physics, &drive, 1);
    }
    let lean = lean(&dom, root);
    assert!(lean < 0.01, "leaning {lean} degrees");
}

/// Measured from 20, 30 and 40 degrees: gone in about 0.1 s, with the root
/// where it was.
#[test]
fn a_leaning_humanoid_rights_itself() {
    let mut dom = Dom::new();
    let mut physics = Physics::new();
    let drive = standing(&mut dom, &mut physics);
    let at = position(&dom, drive.root);
    lean_over(&mut dom, drive.root, 40.0);
    frames(&mut dom, &mut physics, &drive, 6);
    let lean = lean(&dom, drive.root);
    assert!(lean < 0.1, "still leaning {lean} degrees");
    let moved = (position(&dom, drive.root) - at).length();
    assert!(moved < 0.1, "moved {moved} studs");
    assert_eq!(physics.humanoid_state(drive.humanoid), Some(State::Running));
}

/// Measured: 46 degrees is too far. It lies down for 3 s, then stands.
#[test]
fn leaning_past_45_degrees_falls_down_and_gets_up() {
    let mut dom = Dom::new();
    let mut physics = Physics::new();
    let drive = standing(&mut dom, &mut physics);
    lean_over(&mut dom, drive.root, 46.0);
    frames(&mut dom, &mut physics, &drive, 2);
    assert_eq!(
        physics.humanoid_state(drive.humanoid),
        Some(State::FallingDown)
    );
    frames(&mut dom, &mut physics, &drive, 170);
    assert_eq!(
        physics.humanoid_state(drive.humanoid),
        Some(State::FallingDown)
    );
    frames(&mut dom, &mut physics, &drive, 40);
    assert_eq!(physics.humanoid_state(drive.humanoid), Some(State::Running));
    let lean = lean(&dom, drive.root);
    assert!(lean < 0.1, "still leaning {lean} degrees");
}

/// An unanchored 4 × 1 × 4 Seat at height `at`, tilted `degrees` about X,
/// with a sitter welded on as a SeatWeld holds one.
fn loose_seat(dom: &mut Dom, physics: &mut Physics, at: f32, degrees: f32) -> (InstanceId, Drive) {
    let mut drive = standing(dom, physics);
    let at = Vec3::new(0.0, at, 0.0);
    let seat = part_of(dom, "Seat", at, Vec3::new(4.0, 1.0, 4.0), false);
    let tilted = glam::Mat3::from_rotation_x(degrees.to_radians());
    let mut cf = CFrame::from_position(at);
    cf.rotation = tilted;
    set(dom, seat, "CFrame", Value::CFrame(cf));
    let above = CFrame::from_position(Vec3::new(0.0, 2.0, 0.0));
    set(dom, drive.root, "CanCollide", Value::Bool(false));
    set(dom, drive.root, "CFrame", Value::CFrame(cf * above));
    child(
        dom,
        "Weld",
        seat,
        &[
            ("Part0", Value::Ref(Some(seat.to_ref()))),
            ("Part1", Value::Ref(Some(drive.root.to_ref()))),
            ("C0", Value::CFrame(above)),
        ],
    );
    drive.sit = true;
    (seat, drive)
}

/// Measured: a sitter leaves its loose seat to physics: it falls from 12
/// at gravity and the ground, not the Humanoid, decides how it lies, still
/// Seated. Held upright it kept its 20 degrees and stood on its legs.
#[test]
fn a_loose_seat_falls_and_pitches_with_its_sitter() {
    let mut dom = Dom::new();
    let mut physics = Physics::new();
    let (seat, drive) = loose_seat(&mut dom, &mut physics, 12.0, 20.0);
    frames(&mut dom, &mut physics, &drive, 12);
    let tilted = lean(&dom, seat);
    assert!((tilted - 20.0).abs() < 0.5, "tilted {tilted} degrees");
    let fallen = 12.0 - position(&dom, seat).y;
    let gravity = 196.2 * 0.2f32.powi(2) / 2.0;
    assert!((fallen - gravity).abs() < 0.5, "fell {fallen} studs");
    frames(&mut dom, &mut physics, &drive, 120);
    let tilt = lean(&dom, seat);
    assert!((tilt - 20.0).abs() > 5.0, "still tilted {tilt} degrees");
    assert!(position(&dom, seat).y < 5.0, "never landed");
    assert_eq!(physics.humanoid_state(drive.humanoid), Some(State::Seated));
}

/// Measured: FloorMaterial reads Air on a seat, with a floor within the
/// legs' reach under it.
#[test]
fn a_sitter_feels_no_floor() {
    let mut dom = Dom::new();
    let mut physics = Physics::new();
    let (_, drive) = loose_seat(&mut dom, &mut physics, 1.0, 0.0);
    frames(&mut dom, &mut physics, &drive, 30);
    assert_eq!(physics.humanoid_state(drive.humanoid), Some(State::Seated));
    let report = physics.humanoid_reports(&dom).remove(0);
    assert_eq!(report.floor_material, None);
}

/// Measured with no seat at all: the legs let go, it drops onto its root
/// and stays up only because that box sits flat, feeling no floor.
#[test]
fn sitting_on_the_floor_drops_onto_the_root() {
    let mut dom = Dom::new();
    let mut physics = Physics::new();
    let mut drive = standing(&mut dom, &mut physics);
    drive.sit = true;
    frames(&mut dom, &mut physics, &drive, 60);
    assert_eq!(physics.humanoid_state(drive.humanoid), Some(State::Seated));
    let height = position(&dom, drive.root).y;
    assert!((height - 1.5).abs() < 0.05, "root at {height}");
    let lean = lean(&dom, drive.root);
    assert!(lean < 0.1, "leaning {lean} degrees");
    let report = physics.humanoid_reports(&dom).remove(0);
    assert_eq!(report.floor_material, None);
}

/// Measured: leaned past where its box tips, a sitter lies where it falls,
/// still Seated.
#[test]
fn a_sitter_leaned_too_far_lies_down() {
    let mut dom = Dom::new();
    let mut physics = Physics::new();
    let mut drive = standing(&mut dom, &mut physics);
    drive.sit = true;
    frames(&mut dom, &mut physics, &drive, 60);
    lean_over(&mut dom, drive.root, 60.0);
    frames(&mut dom, &mut physics, &drive, 90);
    let lean = lean(&dom, drive.root);
    assert!((lean - 90.0).abs() < 1.0, "leaning {lean} degrees");
    assert_eq!(physics.humanoid_state(drive.humanoid), Some(State::Seated));
}

/// Measured: leaving a sit, the root rises from the floor to standing
/// height in about 0.1 s. Its bottom lies on the floor, where a ray from
/// there feels nothing.
#[test]
fn leaving_a_sit_stands_back_up() {
    let mut dom = Dom::new();
    let mut physics = Physics::new();
    let mut drive = standing(&mut dom, &mut physics);
    drive.sit = true;
    frames(&mut dom, &mut physics, &drive, 60);
    drive.sit = false;
    frames(&mut dom, &mut physics, &drive, 12);
    assert_eq!(physics.humanoid_state(drive.humanoid), Some(State::Running));
    let height = position(&dom, drive.root).y;
    assert!((height - 3.5).abs() < 0.05, "root at {height}");
}

/// Where the root of a Humanoid stood up `above` standing height is, over
/// standing height, after each of `count` 60 Hz frames, under `gravity`.
fn dropped(above: f32, gravity: f32, count: u32) -> Vec<f32> {
    let mut dom = Dom::new();
    let mut physics = Physics::new();
    let drive = standing(&mut dom, &mut physics);
    let ws = dom.find_service("Workspace").unwrap();
    set(&mut dom, ws, "Gravity", Value::Float32(gravity));
    let at = position(&dom, drive.root) + Vec3::Y * above;
    set(
        &mut dom,
        drive.root,
        "CFrame",
        Value::CFrame(CFrame::from_position(at)),
    );
    (0..count)
        .map(|_| {
            frames(&mut dom, &mut physics, &drive, 1);
            position(&dom, drive.root).y - 3.5
        })
        .collect()
}

/// Measured stepping 1.5 studs down: it falls at gravity's pace, unbraked,
/// to standing height in six frames, dips 0.14 below it and comes back up
/// over about 0.15 s without overshooting.
#[test]
fn stepping_down_falls_and_dips() {
    let ys = dropped(1.5, 196.2, 12);
    let dip = ys.iter().copied().fold(0.0, f32::min);
    assert!(ys[4] > 0.0 && ys[5] < 0.0, "{ys:?}");
    assert!(near(dip, -0.143, 0.03), "{ys:?}");
    assert!(near(ys[10], -0.018, 0.01), "{ys:?}");
    assert!(ys.iter().all(|&y| y < 1.5 && y > -0.2), "{ys:?}");
}

/// Measured with gravity off: the legs pull the root down at a steady
/// 2.727 times standing height a second (8.2 for R15's 3.0075).
#[test]
fn without_gravity_the_legs_pull_down_steadily() {
    let ys = dropped(1.5, 0.0, 8);
    let speed = (ys[3] - ys[5]) * 30.0;
    assert!(near(speed, 2.727 * 3.0, 0.05), "{ys:?}");
}

fn near(got: f32, want: f32, within: f32) -> bool {
    (got - want).abs() <= within
}
