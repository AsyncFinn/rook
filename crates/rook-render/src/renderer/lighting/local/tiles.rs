//! Which parts of the light volume to fill again: each layer cut into
//! [`TILE`]-voxel squares, a bit each, so a part that moves or a light that
//! changes refills the squares it can reach, not the whole volume, as
//! voxel lighting engines update their grids in chunks
//! (docs/behavior/lighting/local-lights.md).

use glam::IVec3;

use super::VOLUME;

/// A tile's width along x and z, in voxels.
pub(super) const TILE: u32 = 16;
/// Tiles along x and z.
const ACROSS: [u32; 2] = [VOLUME[0] / TILE, VOLUME[2] / TILE];
/// Words of bits a layer's tiles take.
pub(super) const WORDS: usize = (ACROSS[0] * ACROSS[1]).div_ceil(32) as usize;

/// The tiles to fill, by layer, in the volume's own voxels.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Tiles {
    layers: Vec<[u32; WORDS]>,
}

impl Tiles {
    pub(super) fn none() -> Self {
        Self {
            layers: vec![[0; WORDS]; VOLUME[1] as usize],
        }
    }

    pub(super) fn all() -> Self {
        let mut tiles = Self::none();
        tiles.mark((IVec3::ZERO, volume_high()));
        tiles
    }

    /// Mark the tiles holding any voxel from `lo` to `hi` (both included,
    /// the volume's own voxels); what falls outside the volume is dropped.
    pub(super) fn mark(&mut self, (lo, hi): (IVec3, IVec3)) {
        let (lo, hi) = (lo.max(IVec3::ZERO), hi.min(volume_high()));
        if lo.cmpgt(hi).any() {
            return;
        }
        let tile = TILE as i32;
        let (x0, x1) = (lo.x / tile, hi.x / tile);
        let (z0, z1) = (lo.z / tile, hi.z / tile);
        for layer in &mut self.layers[lo.y as usize..=hi.y as usize] {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let t = (x + ACROSS[0] as i32 * z) as usize;
                    layer[t / 32] |= 1 << (t % 32);
                }
            }
        }
    }

    /// Layer `y`'s tiles, as the fill pass reads them, and the first and
    /// last tile along x and z that are marked; `None` if none are.
    pub(super) fn layer(&self, y: u32) -> Option<(&[u32; WORDS], [u32; 2], [u32; 2])> {
        let bits = &self.layers[y as usize];
        let mut lo = [u32::MAX; 2];
        let mut hi = [0; 2];
        for (w, &word) in bits.iter().enumerate() {
            let mut word = word;
            while word != 0 {
                let t = w as u32 * 32 + word.trailing_zeros();
                word &= word - 1;
                let at = [t % ACROSS[0], t / ACROSS[0]];
                for axis in 0..2 {
                    lo[axis] = lo[axis].min(at[axis]);
                    hi[axis] = hi[axis].max(at[axis]);
                }
            }
        }
        (lo[0] != u32::MAX).then_some((bits, lo, hi))
    }
}

/// The volume's highest voxel.
fn volume_high() -> IVec3 {
    IVec3::new(VOLUME[0] as i32, VOLUME[1] as i32, VOLUME[2] as i32) - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_the_tiles_a_box_reaches_in_its_layers() {
        let mut tiles = Tiles::none();
        assert!(tiles.layer(4).is_none());
        tiles.mark((IVec3::new(15, 3, 40), IVec3::new(16, 4, 40)));
        assert!(tiles.layer(2).is_none());
        let (_, lo, hi) = tiles.layer(4).unwrap();
        assert_eq!((lo, hi), ([0, 2], [1, 2]));
        // Outside the volume, dropped.
        tiles.mark((IVec3::new(-40, 0, 0), IVec3::new(-1, 0, 0)));
        assert!(tiles.layer(0).is_none());
        assert_eq!(Tiles::all().layer(63).unwrap().1, [0, 0]);
    }
}
