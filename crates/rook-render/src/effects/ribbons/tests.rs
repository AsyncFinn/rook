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

/// A Trail lays pieces as its Attachments move, and they die after its
/// Lifetime; `Clear` drops them at once.
#[test]
fn trail_lays_and_ages_pieces() {
    let mut t = Trail {
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
        r.set(&[], std::slice::from_ref(&t), &[]);
        r.step(0.1);
    }
    let strips = r.strips(Vec3::new(0.0, 0.0, 10.0), 0.0);
    assert_eq!(strips[0].edges.len(), 10);
    t.enabled = false;
    r.set(&[], std::slice::from_ref(&t), &[]);
    r.step(1.0);
    assert!(!r.any());
    t.enabled = true;
    r.set(&[], std::slice::from_ref(&t), &[]);
    r.step(0.1);
    t.cleared = 1;
    r.set(&[], std::slice::from_ref(&t), &[]);
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
/// at To, as two quads, their width along the wire from From to To and
/// their height across it as measured: -X and -Z going up, -Y and +Z
/// along +X; CycleOffset and Velocity move them towards To.
#[test]
fn floor_wire_images_count_back_from_to() {
    let (still, offset, moving) = (wire(0.0, 0.0), wire(0.0, 0.25), wire(1.0, 0.0));
    let still = floor_wire(&still, 0.0);
    let centers: Vec<Vec3> = still.chunks(2).map(|pair| center(&pair[0])).collect();
    let near = |a: Vec3, b: Vec3| a.distance(b) < 1e-4;
    assert_eq!(centers.len(), 6);
    assert!(near(centers[5], Vec3::new(10.0, 8.0, 0.0)));
    assert!(near(centers[4], Vec3::new(10.0, 4.0, 0.0)));
    assert!(near(centers[3], Vec3::new(9.8, 0.2, 0.0)));
    assert!(near(centers[0], Vec3::new(0.0, 2.4, 0.0)));
    let up = &still[10..12];
    assert!(near(
        up[0].edges[1].sides[0] - up[0].edges[0].sides[0],
        Vec3::Y * 2.0
    ));
    assert!(near(
        up[0].edges[0].sides[1] - up[0].edges[0].sides[0],
        Vec3::NEG_X
    ));
    assert!(near(
        up[1].edges[0].sides[1] - up[1].edges[0].sides[0],
        Vec3::NEG_Z
    ));
    let along = &still[4..6];
    assert!(near(
        along[0].edges[0].sides[1] - along[0].edges[0].sides[0],
        Vec3::NEG_Y
    ));
    assert!(near(
        along[1].edges[0].sides[1] - along[1].edges[0].sides[0],
        Vec3::Z
    ));
    let offset = floor_wire(&offset, 0.0);
    let moving = floor_wire(&moving, 1.0);
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
