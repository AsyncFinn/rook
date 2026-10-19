//! Where a new particle is born and how fast it goes, along its emitter's
//! axes: ParticleEmitter's `Shape`, `EmissionDirection`, `Speed` and
//! `SpreadAngle` (docs/behavior/particle-emitters.md), and the ball Fire,
//! Smoke and Sparkles emit from.

use glam::{Quat, Vec2, Vec3};

use super::Rng;
use crate::scene::{EmitterLook, EmitterShape, InOut, Spawn};

/// A new particle's place and velocity, along the emitter's axes.
pub(super) fn spawn(look: &EmitterLook, r: &mut Rng) -> (Vec3, Vec3) {
    let (offset, out) = match look.spawn {
        Spawn::Ball(radius) => (r.in_ball() * radius, look.direction),
        Spawn::Shape {
            shape,
            half,
            surface,
            in_out,
            partial,
        } => {
            let (offset, out) = shape_point(shape, half, surface, partial, look.direction, r);
            let flip = match in_out {
                InOut::Outward => false,
                InOut::Inward => true,
                InOut::Both => r.next() < 0.5,
            };
            (offset, if flip { -out } else { out })
        }
    };
    let speed = r.between(look.speed);
    let way = spread(out, look.spread_angle, r);
    let velocity = look.velocity + way * speed + r.in_ball() * look.spread;
    (offset, velocity)
}

/// A point in (or on) the shape, and which way out of it a particle born
/// there goes.
fn shape_point(
    shape: EmitterShape,
    half: Vec3,
    surface: bool,
    partial: f32,
    direction: Vec3,
    r: &mut Rng,
) -> (Vec3, Vec3) {
    match shape {
        EmitterShape::Box => (box_point(half, surface, r), direction),
        EmitterShape::Sphere => {
            // ShapePartial keeps the cap around the direction's pole: 1
            // the whole sphere, 0.5 a half, 0 the pole (creator-docs).
            let floor = 1.0 - 2.0 * partial.clamp(0.0, 1.0);
            let mut unit = r.on_sphere();
            for _ in 0..16 {
                if unit.dot(direction) >= floor {
                    break;
                }
                unit = r.on_sphere();
            }
            let depth = if surface { 1.0 } else { r.next().cbrt() };
            (unit * depth * half, unit)
        }
        EmitterShape::Cylinder => {
            // Its axis lies across the direction (measured: Top gives the
            // part's X axis, Right its Z axis).
            let axis = across(direction);
            let side = direction.cross(axis);
            let length = half.dot(axis).abs();
            let radii = Vec2::new(half.dot(direction).abs(), half.dot(side).abs());
            let angle = r.next() * std::f32::consts::TAU;
            let round = Vec2::new(angle.cos(), angle.sin());
            let depth = if surface { 1.0 } else { r.next().sqrt() };
            let across = round * radii * depth;
            let offset = axis * length * r.signed() + direction * across.x + side * across.y;
            (offset, direction * round.x + side * round.y)
        }
        EmitterShape::Disc => {
            // On the face the direction points to (measured), its inner
            // radius ShapePartial of the outer.
            let u = across(direction);
            let v = direction.cross(u);
            let radii = Vec2::new(half.dot(u).abs(), half.dot(v).abs());
            let inner = partial.clamp(0.0, 1.0);
            let depth = if surface {
                1.0
            } else {
                (inner * inner + (1.0 - inner * inner) * r.next()).sqrt()
            };
            let angle = r.next() * std::f32::consts::TAU;
            let at = Vec2::new(angle.cos(), angle.sin()) * radii * depth;
            let face = direction * half.dot(direction).abs();
            (face + u * at.x + v * at.y, direction)
        }
    }
}

/// A point in the box of half-size `half`, or on its faces (each as
/// likely as its area).
fn box_point(half: Vec3, surface: bool, r: &mut Rng) -> Vec3 {
    let inside = Vec3::new(r.signed(), r.signed(), r.signed()) * half;
    if !surface {
        return inside;
    }
    let areas = [half.y * half.z, half.x * half.z, half.x * half.y];
    let total: f32 = areas.iter().sum();
    if total <= 0.0 {
        return inside;
    }
    let mut pick = r.next() * total;
    let mut axis = 2;
    for (i, a) in areas.iter().enumerate() {
        if pick < *a {
            axis = i;
            break;
        }
        pick -= a;
    }
    let mut p = inside;
    p[axis] = if r.next() < 0.5 {
        -half[axis]
    } else {
        half[axis]
    };
    p
}

/// A unit axis across `direction`: the right of a view looking along it
/// with up as up (X when it points up or down).
fn across(direction: Vec3) -> Vec3 {
    let right = direction.cross(Vec3::Y);
    if right.length_squared() < 1e-6 {
        Vec3::X
    } else {
        right.normalize()
    }
}

/// `way` turned at random by up to `angles` degrees about the two axes
/// across it (for a Top emitter, X then Z).
fn spread(way: Vec3, angles: Vec2, r: &mut Rng) -> Vec3 {
    if angles == Vec2::ZERO || way == Vec3::ZERO {
        return way;
    }
    let (u, v) = if way.cross(Vec3::Y).length_squared() < 1e-6 {
        (Vec3::X, Vec3::Z)
    } else {
        let u = Vec3::Y.cross(way).normalize();
        (u, way.cross(u))
    };
    let a = (r.signed() * angles.x).to_radians();
    let b = (r.signed() * angles.y).to_radians();
    Quat::from_axis_angle(u, a) * (Quat::from_axis_angle(v, b) * way)
}
