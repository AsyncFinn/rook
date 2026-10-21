//! A FloorWire's images as strips: each two crossed quads on its wire
//! (docs/behavior/adornments.md).

use std::f32::consts::PI;

use glam::Quat;

use super::*;

/// Every image of `w` at `time` seconds, two strips each.
pub(super) fn floor_wire(w: &FloorWire, time: f32) -> Vec<Strip<'_>> {
    let lengths: Vec<f32> = w.path.windows(2).map(|l| l[0].distance(l[1])).collect();
    let total: f32 = lengths.iter().sum();
    if w.spacing <= 0.0 || total <= 0.0 || w.look.texture.is_none() {
        return Vec::new();
    }
    let ahead = w.cycle_offset * w.spacing + w.velocity * time;
    let alpha = w.look.alpha.at(0.0, |_| 0.0).clamp(0.0, 1.0);
    let mut out = Vec::new();
    let (mut leg, mut start) = (0, 0.0);
    let mut at = (total + ahead).rem_euclid(w.spacing);
    while at <= total {
        while leg + 1 < lengths.len() && at > start + lengths[leg] {
            start += lengths[leg];
            leg += 1;
        }
        let way = (w.path[leg + 1] - w.path[leg]).normalize_or_zero();
        if way != Vec3::ZERO {
            let centre = w.path[leg] + way * (at - start);
            let turn = turn(way);
            for across in [turn * Vec3::NEG_X, turn * Vec3::Z] {
                out.push(quad(w, centre, way, across, alpha));
            }
        }
        at += w.spacing;
    }
    out
}

/// The turn taking -Y onto `way` by the shortest arc, and about X when
/// `way` is straight up: the image's across axes are then -X and +Z turned
/// (measured on every leg direction of a wire).
fn turn(way: Vec3) -> Quat {
    if way.y > 0.9999 {
        Quat::from_rotation_x(PI)
    } else {
        Quat::from_rotation_arc(Vec3::NEG_Y, way)
    }
}

/// One quad of the image at `centre`: its width along `way` from u 0 to 1,
/// its height along `across` from v 0 to 1.
fn quad<'a>(w: &'a FloorWire, centre: Vec3, way: Vec3, across: Vec3, alpha: f32) -> Strip<'a> {
    let half = w.size * 0.5;
    let edge = |u: f32| {
        let at = centre + way * (half.x * (2.0 * u - 1.0));
        Edge {
            sides: [at - across * half.y, at + across * half.y],
            u,
            color: w.look.color.at(0.0),
            alpha,
        }
    };
    Strip {
        look: &w.look,
        edges: vec![edge(0.0), edge(1.0)],
    }
}
