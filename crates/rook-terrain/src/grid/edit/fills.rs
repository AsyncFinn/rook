//! Fills: a shape or block filled with a material, merged into each
//! voxel by [`merge`]'s rules, and replacing one material with another.

use super::*;

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
}
