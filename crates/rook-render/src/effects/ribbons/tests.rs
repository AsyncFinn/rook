use glam::Mat3;

use super::*;
use crate::scene::{Anchor, Curve, Sequence};

fn look() -> RibbonLook {
    RibbonLook {
        texture: None,
        texture_mode: TextureMode::Stretch,
        texture_length: 1.0,
        color: Curve::constant([1.0; 3]),
        alpha: Sequence::constant(1.0),
        brightness: 1.0,
        light_emission: 0.0,
        light_influence: 0.0,
        face_camera: false,
    }
}

/// A straight Beam between Attachments at their default orientation is
/// as wide as its widths along their up (measured), in `Segments` pieces.
#[test]
fn beam_width_runs_along_attachment_up() {
    let anchor = |x| Anchor {
        position: Vec3::new(x, 0.0, 0.0),
        rotation: Mat3::IDENTITY,
    };
    let b = Beam {
        enabled: true,
        ends: [anchor(-6.0), anchor(6.0)],
        curve: [0.0, 0.0],
        width: [3.0, 1.0],
        segments: 10,
        texture_speed: 0.0,
        texture_offset: 0.0,
        z_offset: 0.0,
        look: look(),
    };
    let s = beam(&b, Vec3::new(0.0, 0.0, 30.0), 0.0).unwrap();
    assert_eq!(s.edges.len(), 11);
    let first = s.edges[0].sides;
    assert_eq!(
        first,
        [Vec3::new(-6.0, -1.5, 0.0), Vec3::new(-6.0, 1.5, 0.0)]
    );
    assert!((s.edges[10].sides[1].y - 0.5).abs() < 1e-5);
}

/// Measured in Studio with an asymmetric image on a 20-stud Beam facing
/// the camera: its rows run along the beam from Attachment1 (row 0) to
/// Attachment0 (row 1), its columns across it, rising towards the
/// screen's top.
#[test]
fn beam_rows_run_back_from_attachment1() {
    let anchor = |x| Anchor {
        position: Vec3::new(x, 0.0, 0.0),
        rotation: Mat3::IDENTITY,
    };
    let b = Beam {
        enabled: true,
        ends: [anchor(0.0), anchor(20.0)],
        curve: [0.0, 0.0],
        width: [6.0, 6.0],
        segments: 4,
        texture_speed: 0.0,
        texture_offset: 0.0,
        z_offset: 0.0,
        look: RibbonLook {
            face_camera: true,
            ..look()
        },
    };
    let s = beam(&b, Vec3::new(10.0, 0.0, -18.0), 0.0).unwrap();
    assert_eq!(s.along, ImageAxis::Y);
    let (a0, a1) = (s.edges[0], s.edges[4]);
    // Rows repeat each 1: Attachment0's is Attachment1's plus one.
    assert!((a0.u - a1.u - 1.0).abs() < 1e-5, "{} {}", a0.u, a1.u);
    assert!((s.edges[1].u - a1.u - 0.75).abs() < 1e-5);
    assert!(a0.sides[1].y > a0.sides[0].y);
}

/// Measured likewise on a Trail laid left to right: its rows run from the
/// tail (row 0) to the newest end, its columns from Attachment0 to
/// Attachment1.
#[test]
fn trail_rows_run_from_the_tail() {
    let mut t = Trail {
        key: 1,
        ends: [Vec3::ZERO, Vec3::Y],
        enabled: true,
        lifetime: 10.0,
        min_length: 0.1,
        max_length: 0.0,
        width_scale: Sequence::constant(1.0),
        cleared: 0,
        look: look(),
    };
    let mut r = Ribbons::default();
    for i in 0..=4 {
        t.ends = [Vec3::new(i as f32, 0.0, 0.0), Vec3::new(i as f32, 6.0, 0.0)];
        r.set_trail(0, Some(&t));
        r.step(0.1);
    }
    let strips = r.strips(Vec3::new(2.0, 3.0, -18.0), 0.0);
    let s = &strips[0];
    assert_eq!(s.along, ImageAxis::Y);
    let (newest, tail) = (s.edges[0], s.edges[s.edges.len() - 1]);
    assert_eq!(newest.sides[0].x, 4.0);
    assert!(
        (newest.u - tail.u - 1.0).abs() < 1e-5,
        "{} {}",
        newest.u,
        tail.u
    );
    assert_eq!(newest.sides[0].y, 0.0);
}

/// A Trail lays pieces as its Attachments move, and they die after its
/// Lifetime; `Clear` drops them at once.
#[test]
fn trail_lays_and_ages_pieces() {
    let mut t = Trail {
        key: 1,
        ends: [Vec3::ZERO, Vec3::Y],
        enabled: true,
        lifetime: 1.0,
        min_length: 0.1,
        max_length: 0.0,
        width_scale: Sequence::constant(1.0),
        cleared: 0,
        look: look(),
    };
    let mut r = Ribbons::default();
    for i in 0..10 {
        t.ends = [Vec3::new(i as f32, 0.0, 0.0), Vec3::new(i as f32, 1.0, 0.0)];
        r.set_trail(0, Some(&t));
        r.step(0.1);
    }
    let strips = r.strips(Vec3::new(0.0, 0.0, 10.0), 0.0);
    assert_eq!(strips[0].edges.len(), 10);
    t.enabled = false;
    r.set_trail(0, Some(&t));
    r.step(1.0);
    assert!(!r.any());
    t.enabled = true;
    r.set_trail(0, Some(&t));
    r.step(0.1);
    t.cleared = 1;
    r.set_trail(0, Some(&t));
    assert!(!r.any());
}

fn wire(velocity: f32, cycle_offset: f32) -> FloorWire {
    FloorWire {
        path: vec![
            Vec3::new(0.0, 6.0, 0.0),
            Vec3::new(0.0, 0.2, 0.0),
            Vec3::new(10.0, 0.2, 0.0),
            Vec3::new(10.0, 8.0, 0.0),
        ],
        size: glam::Vec2::new(2.0, 1.0),
        spacing: 4.0,
        velocity,
        cycle_offset,
        look: RibbonLook {
            texture: Some(crate::ImageHash([0; 32])),
            ..look()
        },
    }
}

/// The middle of a strip's first edge.
fn center(s: &Strip<'_>) -> Vec3 {
    (s.edges[0].sides[0] + s.edges[1].sides[1]) * 0.5
}

/// A FloorWire's images sit every StudsBetweenTextures back from its end
/// at To, one quad each, its width along the wire from From to To;
/// CycleOffset and Velocity move them towards To.
#[test]
fn floor_wire_images_count_back_from_to() {
    let (still, offset, moving) = (wire(0.0, 0.0), wire(0.0, 0.25), wire(1.0, 0.0));
    let camera = Vec3::new(5.0, 4.0, 30.0);
    let still = floor_wire(&still, camera, 0.0);
    let centers: Vec<Vec3> = still.iter().map(center).collect();
    let near = |a: Vec3, b: Vec3| a.distance(b) < 1e-4;
    assert_eq!(centers.len(), 6);
    assert!(near(centers[5], Vec3::new(10.0, 8.0, 0.0)));
    assert!(near(centers[4], Vec3::new(10.0, 4.0, 0.0)));
    assert!(near(centers[3], Vec3::new(9.8, 0.2, 0.0)));
    assert!(near(centers[0], Vec3::new(0.0, 2.4, 0.0)));
    assert!(near(
        still[5].edges[1].sides[0] - still[5].edges[0].sides[0],
        Vec3::Y * 2.0
    ));
    let offset = floor_wire(&offset, camera, 0.0);
    let moving = floor_wire(&moving, camera, 1.0);
    // One stud on: the last image passes the end and the one before is next.
    assert!(near(
        center(offset.last().unwrap()),
        Vec3::new(10.0, 5.0, 0.0)
    ));
    assert!(near(
        center(moving.last().unwrap()),
        Vec3::new(10.0, 5.0, 0.0)
    ));
}

/// Measured: a FloorWire image turns about its wire to face the camera,
/// keeping to the side of its rest axis (-X on an upright leg, down on a
/// level one), so it flips over as the camera crosses that plane.
#[test]
fn floor_wire_images_face_the_camera() {
    let w = wire(0.0, 0.0);
    let across = |camera: Vec3, i: usize| {
        let s = &floor_wire(&w, camera, 0.0)[i];
        (s.edges[0].sides[1] - s.edges[0].sides[0]).normalize()
    };
    let near = |a: Vec3, b: Vec3| a.distance(b) < 1e-3;
    // The upright leg's top image, at (10, 8, 0).
    assert!(near(across(Vec3::new(10.0, 8.0, 20.0), 5), Vec3::NEG_X));
    assert!(across(Vec3::new(30.0, 8.0, 1.0), 5).z > 0.99);
    assert!(across(Vec3::new(30.0, 8.0, -1.0), 5).z < -0.99);
    // A level leg's image, at (5.8, 0.2, 0), seen from 45 degrees above.
    let slope = std::f32::consts::FRAC_1_SQRT_2;
    let front = Vec3::new(0.0, -slope, slope);
    assert!(near(across(Vec3::new(5.8, 10.2, 10.0), 2), front));
    assert!(near(
        across(Vec3::new(5.8, 10.2, -10.0), 2),
        front * Vec3::new(1.0, 1.0, -1.0)
    ));
}
