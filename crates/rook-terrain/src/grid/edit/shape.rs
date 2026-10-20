//! The shapes scripts fill (`FillBlock`, `FillBall`, `FillCylinder`,
//! `FillWedge`): how much of each voxel near one they cover.

use glam::{Affine3A, Vec3};

use super::*;

/// Half a voxel.
const HALF: f32 = VOXEL_STUDS / 2.0;

/// A shape scripts fill with a material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// `FillBlock`: a box of `size` at `pose`.
    Block { pose: Affine3A, size: Vec3 },
    /// `FillBall`.
    Ball { center: Vec3, radius: f32 },
    /// `FillCylinder`: round in the pose's XZ plane, `height` along its Y.
    Cylinder {
        pose: Affine3A,
        height: f32,
        radius: f32,
    },
    /// `FillWedge`: a WedgePart's shape, full height at +Z and sloping to
    /// nothing at -Z.
    Wedge { pose: Affine3A, size: Vec3 },
}

/// How much of a voxel's span `[p - 2, p + 2]` the span `[-h, h]` covers,
/// from 0 to 1.
fn overlap(p: f32, h: f32) -> f32 {
    ((p + HALF).min(h) - (p - HALF).max(-h)).clamp(0.0, VOXEL_STUDS) / VOXEL_STUDS
}

/// Whether a voxel's span around `p` reaches into `[-h, h]` at all.
fn touches(p: f32, h: f32) -> bool {
    p.abs() - HALF < h
}

fn round_byte(f: f32) -> u8 {
    (f.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl Shape {
    /// Whether the shape has no size (Studio: "Extents cannot be empty").
    pub fn is_empty(&self) -> bool {
        match *self {
            Shape::Block { size, .. } | Shape::Wedge { size, .. } => !size.cmpgt(Vec3::ZERO).all(),
            Shape::Ball { radius, .. } => radius.is_nan() || radius <= 0.0,
            Shape::Cylinder { height, radius, .. } => {
                height.is_nan() || radius.is_nan() || height <= 0.0 || radius <= 0.0
            }
        }
    }

    /// How full of the shape the voxel centred at `c` is, as a byte.
    ///
    /// - Block: the least of the three overlaps along the box's own axes
    ///   (a 1-stud cube in a voxel fills a quarter of it, not 1/64; a box
    ///   turned 45° fills its corner neighbours by 22 of 255).
    /// - Ball: `(r - d) / 4 + 1/2` of the distance `d` from the centre.
    /// - Cylinder: `(r - d) / 4` of the distance from its axis, rounded
    ///   down, where the voxel reaches into its height at all.
    /// - Wedge: across, as a block; along its slope, `(t - y) / 4 + 1/2`
    ///   where `t` is the slope's height a half voxel towards its low end;
    ///   wherever the voxel reaches into its bottom and depth.
    pub(super) fn occupancy(&self, c: Vec3) -> u8 {
        match *self {
            Shape::Block { pose, size } => {
                let p = pose.inverse().transform_point3(c);
                let h = size * 0.5;
                round_byte(
                    overlap(p.x, h.x)
                        .min(overlap(p.y, h.y))
                        .min(overlap(p.z, h.z)),
                )
            }
            Shape::Ball { center, radius } => {
                round_byte((radius - c.distance(center)) / VOXEL_STUDS + 0.5)
            }
            Shape::Cylinder {
                pose,
                height,
                radius,
            } => {
                let p = pose.inverse().transform_point3(c);
                if !touches(p.y, height * 0.5) {
                    return 0;
                }
                let d = p.x.hypot(p.z);
                (((radius - d) / VOXEL_STUDS).clamp(0.0, 1.0) * 255.0) as u8
            }
            Shape::Wedge { pose, size } => {
                let p = pose.inverse().transform_point3(c);
                let h = size * 0.5;
                if !touches(p.y, h.y) || !touches(p.z, h.z) {
                    return 0;
                }
                let top = -h.y + size.y * (p.z - HALF + h.z) / size.z;
                let slope = (top - p.y) / VOXEL_STUDS + 0.5;
                round_byte(overlap(p.x, h.x).min(slope))
            }
        }
    }

    /// The voxels the shape fills.
    pub(super) fn bounds(&self) -> Bounds {
        let (center, half) = match *self {
            Shape::Block { pose, size } | Shape::Wedge { pose, size } => {
                (pose.translation.into(), abs_extent(pose, size * 0.5))
            }
            Shape::Ball { center, radius } => (center, Vec3::splat(radius)),
            Shape::Cylinder {
                pose,
                height,
                radius,
            } => (
                pose.translation.into(),
                abs_extent(pose, Vec3::new(radius, height * 0.5, radius)),
            ),
        };
        // Voxels reaching into its world box, no further (observed: a box
        // turned 45° fills nothing beyond that box, though its overlap
        // rule would give the voxels just outside it a little).
        Bounds {
            min: ((center - half) / VOXEL_STUDS).floor().as_ivec3(),
            max: ((center + half) / VOXEL_STUDS).floor().as_ivec3(),
        }
    }
}

/// The world box half-extents of a box of half-extents `h` at `pose`.
fn abs_extent(pose: Affine3A, h: Vec3) -> Vec3 {
    let m = pose.matrix3;
    (m.x_axis.abs() * h.x + m.y_axis.abs() * h.y + m.z_axis.abs() * h.z).into()
}
