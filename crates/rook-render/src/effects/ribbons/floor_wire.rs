//! A FloorWire's images as strips: each one quad on its wire, turned
//! about it to the camera (docs/behavior/rendering/adornments.md).

use super::*;

/// Every image of `w` seen from `camera` at `time` seconds, a strip each.
pub(super) fn floor_wire(w: &FloorWire, camera: Vec3, time: f32) -> Vec<Strip<'_>> {
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
            let center = w.path[leg] + way * (at - start);
            let across = across(way, center, camera);
            out.push(quad(w, center, way, across, alpha));
        }
        at += w.spacing;
    }
    out
}

/// The image's height axis at `center`: across `way`, facing `camera`,
/// on the side of its rest axis (down, or -X on an upright leg), so the
/// image flips over as the camera crosses the plane of the wire and that
/// axis (measured).
fn across(way: Vec3, center: Vec3, camera: Vec3) -> Vec3 {
    let down = Vec3::NEG_Y + way * way.y;
    let rest = if down.length_squared() < 1e-6 {
        Vec3::NEG_X
    } else {
        down.normalize()
    };
    let side = way.cross(camera - center);
    if side.length_squared() < 1e-10 {
        return rest;
    }
    let side = side.normalize();
    if side.dot(rest) < 0.0 { -side } else { side }
}

/// One quad of the image at `center`: its width along `way` from u 0 to 1,
/// its height along `across` from v 0 to 1.
fn quad<'a>(w: &'a FloorWire, center: Vec3, way: Vec3, across: Vec3, alpha: f32) -> Strip<'a> {
    let half = w.size * 0.5;
    let edge = |u: f32| {
        let at = center + way * (half.x * (2.0 * u - 1.0));
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
        along: ImageAxis::X,
    }
}
