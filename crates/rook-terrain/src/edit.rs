//! Scripts' edits to the voxels: `Terrain`'s fills, region writes and
//! copies, with the occupancy rules measured in Studio 0.742
//! (`docs/behavior/terrain.md`, "The Terrain API").
//!
//! A fill gives each voxel near the shape an occupancy byte (255 full)
//! from where the voxel's centre lies, then merges it into the voxel: a
//! solid material takes the voxel when it fills at least as much as what
//! is there, water only fills what no solid does, and Air carves.

use glam::{Affine3A, IVec3, Vec3};

use crate::TerrainMaterial;
use crate::grid::{Grid, VOXEL_STUDS, Voxel, chunk_of};

/// Half a voxel.
const HALF: f32 = VOXEL_STUDS / 2.0;

/// The most voxels a region read or write may cover (observed: 4,194,304
/// is allowed, 256 × 256 × 65 is "Region is too large").
pub const REGION_MAX_VOXELS: i64 = 1 << 22;

/// Voxels from `min` to `max`, both inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub min: IVec3,
    pub max: IVec3,
}

impl Bounds {
    /// The voxels from `min` for `size`, or `None` when that's empty.
    pub fn of_block(min: IVec3, size: IVec3) -> Option<Bounds> {
        (size.cmpgt(IVec3::ZERO).all()).then(|| Bounds {
            min,
            max: min + size - IVec3::ONE,
        })
    }

    /// The chunks holding a voxel within `reach` voxels of these.
    pub fn chunks(self, reach: i32) -> impl Iterator<Item = IVec3> {
        let (lo, _) = chunk_of(self.min - reach);
        let (hi, _) = chunk_of(self.max + reach);
        Bounds { min: lo, max: hi }.voxels()
    }

    fn voxels(self) -> impl Iterator<Item = IVec3> {
        let Bounds { min, max } = self;
        (min.z..=max.z).flat_map(move |z| {
            (min.y..=max.y).flat_map(move |y| (min.x..=max.x).map(move |x| IVec3::new(x, y, z)))
        })
    }
}

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
    fn occupancy(&self, c: Vec3) -> u8 {
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
    fn bounds(&self) -> Bounds {
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

/// The voxel after filling `occupancy` of it with `material`:
///
/// - a solid material replaces what is there when it fills at least as
///   much (observed: Rock filling 128 over Grass 128 is Rock; Rock 128 in
///   full water leaves the water), keeping water that was there as the
///   voxel's water (Inference);
/// - water fills air and water, and only the water channel of a solid
///   voxel (observed: a full water block over half-filled Grass leaves
///   the Grass as it was);
/// - Air takes away its occupancy (observed: half a voxel of Air on full
///   Grass leaves 127), clearing the voxel when nothing is left.
pub fn merge(old: Voxel, material: TerrainMaterial, occupancy: u8) -> Voxel {
    if occupancy == 0 {
        return old;
    }
    match material {
        TerrainMaterial::Air => {
            if old.material == TerrainMaterial::Air {
                return old;
            }
            let left = 255 - occupancy;
            if old.occupancy.min(left) == 0 {
                return Voxel::AIR;
            }
            Voxel {
                occupancy: old.occupancy.min(left),
                water: old.water.min(left),
                ..old
            }
        }
        TerrainMaterial::Water if old.is_solid() => Voxel {
            water: old.water.max(occupancy),
            ..old
        },
        TerrainMaterial::Water => Voxel::new(
            TerrainMaterial::Water,
            occupancy.max(if old.material == TerrainMaterial::Water {
                old.occupancy
            } else {
                0
            }),
        ),
        _ if old.material == TerrainMaterial::Air || occupancy >= old.occupancy => Voxel {
            material,
            occupancy,
            water: if old.material == TerrainMaterial::Water {
                old.occupancy
            } else {
                old.water
            },
        },
        _ => old,
    }
}

/// The voxels a `Region3` from `min` to `max` covers for region methods:
/// its corners in voxels, truncated toward zero (observed: -1.9 to 2.1
/// covers no voxel, 1 to 8 covers voxels 0 and 1, -1 to 5 voxel 0).
/// Returns the first voxel and how many on each axis (0 when inverted).
pub fn region_voxels(min: Vec3, max: Vec3) -> (IVec3, IVec3) {
    let lo = (min / VOXEL_STUDS).trunc().as_ivec3();
    let hi = (max / VOXEL_STUDS).trunc().as_ivec3();
    (lo, (hi - lo).max(IVec3::ZERO))
}

impl Grid {
    /// Fill `shape` with `material`; the voxels that may have changed.
    pub fn fill(&mut self, shape: &Shape, material: TerrainMaterial) -> Option<Bounds> {
        if shape.is_empty() {
            return None;
        }
        let bounds = shape.bounds();
        let mut changed = false;
        for v in bounds.voxels() {
            let center = (v.as_vec3() + 0.5) * VOXEL_STUDS;
            let occupancy = shape.occupancy(center);
            if occupancy == 0 {
                continue;
            }
            let old = self.voxel(v);
            let new = merge(old, material, occupancy);
            if new != old {
                self.set(v, new);
                changed = true;
            }
        }
        changed.then_some(bounds)
    }

    /// `FillRegion`: every voxel of the block full of `material` (observed:
    /// water fills half-filled Grass's voxel whole).
    pub fn fill_block(&mut self, bounds: Bounds, material: TerrainMaterial) {
        let voxel = Voxel::new(material, 255);
        for v in bounds.voxels() {
            self.set(v, voxel);
        }
    }

    /// `ReplaceMaterial`: voxels of `from` become `to`, as full as they
    /// were (observed: half-full Grass becomes half-full Water); `to` Air
    /// clears them, and Air becomes full `to` (observed). Returns whether
    /// any changed.
    pub fn replace(&mut self, bounds: Bounds, from: TerrainMaterial, to: TerrainMaterial) -> bool {
        let mut changed = false;
        for v in bounds.voxels() {
            let old = self.voxel(v);
            if old.material != from || from == to {
                continue;
            }
            let new = match to {
                TerrainMaterial::Air => Voxel::AIR,
                _ if from == TerrainMaterial::Air => Voxel::new(to, 255),
                _ => Voxel {
                    material: to,
                    water: if to == TerrainMaterial::Water {
                        0
                    } else {
                        old.water
                    },
                    ..old
                },
            };
            self.set(v, new);
            changed = true;
        }
        changed
    }

    /// Write `voxels` (X fastest, then Y, then Z, as [`Grid::read_block`]
    /// reads them) into the block.
    pub fn write_block(&mut self, bounds: Bounds, voxels: &[Voxel]) {
        for (v, voxel) in bounds.voxels().zip(voxels) {
            self.set(v, *voxel);
        }
    }

    /// The block's voxels as a grid of their own, starting at voxel 0
    /// (`CopyRegion`).
    pub fn copy(&self, bounds: Bounds) -> Grid {
        let mut out = Grid::new();
        for v in bounds.voxels() {
            let voxel = self.voxel(v);
            if voxel != Voxel::AIR {
                out.set(v - bounds.min, voxel);
            }
        }
        out
    }

    /// Put `size` voxels of a copied grid at `corner` (`PasteRegion`);
    /// its air replaces what is there only with `paste_empty` (observed).
    pub fn paste(&mut self, from: &Grid, size: IVec3, corner: IVec3, paste_empty: bool) {
        let Some(source) = Bounds::of_block(IVec3::ZERO, size) else {
            return;
        };
        for v in source.voxels() {
            let voxel = from.voxel(v);
            if paste_empty || voxel.material != TerrainMaterial::Air {
                self.set(corner + v, voxel);
            }
        }
    }

    /// How many voxels aren't air (`CountCells`).
    pub fn count_cells(&self) -> u64 {
        self.counts().iter().sum()
    }
}

#[cfg(test)]
mod tests;
