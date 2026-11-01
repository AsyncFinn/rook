//! The Humanoid controller in a world of its own (Rook's controller; the
//! numbers fitted to Studio are checked in rook-engine's movement tests).

use super::*;
use crate::humanoid::Drive;

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
    let root = part(&mut dom, Vec3::new(0.0, 3.5, 2.0), Vec3::new(2.0, 2.0, 1.0), false);
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
