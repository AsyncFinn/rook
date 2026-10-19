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
        r.set(&[], std::slice::from_ref(&t));
        r.step(0.1);
    }
    let strips = r.strips(Vec3::new(0.0, 0.0, 10.0), 0.0);
    assert_eq!(strips[0].edges.len(), 10);
    t.enabled = false;
    r.set(&[], std::slice::from_ref(&t));
    r.step(1.0);
    assert!(!r.any());
    t.enabled = true;
    r.set(&[], std::slice::from_ref(&t));
    r.step(0.1);
    t.cleared = 1;
    r.set(&[], std::slice::from_ref(&t));
    assert!(!r.any());
}
