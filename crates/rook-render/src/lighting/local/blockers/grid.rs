//! The marked voxels, kept anchored to the world as the volume moves, and
//! the work of marking them a budget at a time on the host's workers.

use super::*;

/// The marked voxels of a volume, anchored to the world: voxel `k`
/// (centered at `4k + 2` on each axis) lives at slot `k mod dims`, so a
/// volume that moves keeps the voxels it still covers and marks only those
/// that came in, a few pieces a frame.
#[derive(Debug)]
pub(super) struct Grid {
    /// The volume's lowest voxel.
    pub(super) low: IVec3,
    pub(super) dims: IVec3,
    /// Blockers covering each voxel.
    pub(super) counts: Vec<u32>,
    /// A bit per voxel covered at all, as the fill pass reads it.
    pub(super) bits: Vec<u32>,
    /// The same bits by column ([`column_of`]), a bit per
    /// layer slot: which layers of each column are blocked, so the fill
    /// can tell where the sky's light spreads sideways.
    pub(super) layers: Vec<u64>,
    /// How full of solid terrain each voxel is (`terrain::soil`), a byte
    /// per voxel, four to a word.
    pub(super) soils: Vec<u32>,
    /// Which layers of each column hold some terrain, as `layers` for the
    /// parts.
    pub(super) soil_layers: Vec<u64>,
    /// Pieces of the volume still to mark since it moved.
    pub(super) pending: Vec<Span>,
}

/// What a grid's voxels are marked from.
#[derive(Debug, Clone, Copy)]
pub(super) struct Sources<'a> {
    pub(super) blockers: &'a [Option<Blocker>],
    pub(super) columns: &'a Columns,
    pub(super) terrain: &'a Terrain,
}

impl Grid {
    /// Marked whole, its bricks at once on the host's workers.
    pub(super) fn new(low: IVec3, dims: IVec3, from: Sources<'_>) -> Self {
        let mut grid = Self::empty(low, dims);
        grid.pending = pieces(grid.span()).collect();
        grid.work(Duration::MAX, from);
        grid
    }

    pub(super) fn empty(low: IVec3, dims: IVec3) -> Self {
        let voxels = (dims.x * dims.y * dims.z) as usize;
        assert!(dims.y <= 64, "a column's layers take a bit each of a u64");
        assert!(dims.x % PIECE == 0 && dims.z % PIECE == 0, "whole bricks");
        Self {
            low,
            dims,
            counts: vec![0; voxels],
            bits: vec![0; voxels.div_ceil(32)],
            layers: vec![0; (dims.x * dims.z) as usize],
            soils: vec![0; voxels.div_ceil(4)],
            soil_layers: vec![0; (dims.x * dims.z) as usize],
            pending: Vec::new(),
        }
    }

    /// All of its marks, to mark one brick after another.
    #[cfg(test)]
    pub(super) fn voxels(&mut self) -> Voxels<'_> {
        Voxels {
            dims: self.dims,
            base: (0, 0),
            counts: &mut self.counts,
            bits: &mut self.bits,
            layers: &mut self.layers,
            soils: &mut self.soils,
            soil_layers: &mut self.soil_layers,
        }
    }

    pub(super) fn span(&self) -> Span {
        (self.low, self.low + self.dims - 1)
    }

    /// Move the volume to start at `low`, leaving the voxels that came in
    /// to [`Grid::work`]. Returns their pieces.
    pub(super) fn move_to(&mut self, low: IVec3) -> Vec<Span> {
        let old = self.span();
        self.low = low;
        let new = self.span();
        // Pieces still to mark from an earlier move keep the part of them
        // still inside; what came in now is outside the old volume, so
        // apart from them.
        self.pending.retain_mut(|span| match clip(*span, new) {
            Some(within) => {
                *span = within;
                true
            }
            None => false,
        });
        // The voxels that came in, as up to three boxes: those past the
        // old volume along x, then along y among the rest, then along z.
        let came: Vec<Span> = minus(new, Some(old)).into_iter().flat_map(pieces).collect();
        self.pending.extend_from_slice(&came);
        came
    }

    /// Mark pending pieces for up to `budget`; whether none are left.
    /// Each piece lies in one brick, and each brick's pieces are marked in
    /// it at once on the host's workers, as engines voxelize their
    /// clipmaps' new bricks as parallel jobs; those not started by then
    /// wait.
    pub(super) fn work(&mut self, budget: Duration, from: Sources<'_>) -> bool {
        if self.pending.is_empty() {
            return true;
        }
        let deadline = Instant::now().checked_add(budget);
        let mut by_brick = vec![Vec::new(); self.bricks()];
        for span in std::mem::take(&mut self.pending) {
            by_brick[self.brick_of(span)].push(span);
        }
        let mut jobs = self.jobs(by_brick);
        let workers = &rook_platform::get().workers;
        workers.for_each_chunk(&mut jobs, 1, |_, job| {
            let mut scratch = Scratch::default();
            for (voxels, spans, done) in job {
                if deadline.is_none_or(|d| Instant::now() < d) {
                    for &span in spans.iter() {
                        voxels.clear(span);
                        voxels.take(span, from, &mut scratch);
                    }
                    *done = true;
                }
            }
        });
        let left: Vec<Span> = (jobs.into_iter())
            .filter(|(_, _, done)| !done)
            .flat_map(|(_, spans, _)| spans)
            .collect();
        self.pending = left;
        self.pending.is_empty()
    }

    /// Count each of `moves` (a blocker, in or out, and the voxels of it
    /// to count) in or out, the bricks at once on the host's workers, each
    /// brick's in the order given: the boxes parts moved from and to, as
    /// voxel engines revoxelize what moved in parallel jobs by brick.
    pub(super) fn apply_moves(&mut self, moves: &[(Blocker, bool, Span)]) {
        let mut by_brick = vec![Vec::new(); self.bricks()];
        for (n, &(_, add, span)) in moves.iter().enumerate() {
            for piece in pieces(span) {
                by_brick[self.brick_of(piece)].push((n, add, piece));
            }
        }
        let mut jobs = self.jobs(by_brick);
        let workers = &rook_platform::get().workers;
        workers.for_each_chunk(&mut jobs, 1, |_, job| {
            for (voxels, list, _) in job {
                for &(n, add, piece) in list.iter() {
                    let b = &moves[n].0;
                    voxels.apply(b, add, piece);
                }
            }
        });
    }

    /// Mark the terrain in `spans` again from `terrain` as it is now, the
    /// bricks at once on the host's workers.
    pub(super) fn apply_soils(&mut self, spans: &[Span], terrain: &Terrain) {
        let mut by_brick = vec![Vec::new(); self.bricks()];
        for &span in spans {
            for piece in pieces(span) {
                by_brick[self.brick_of(piece)].push(piece);
            }
        }
        let mut jobs = self.jobs(by_brick);
        let workers = &rook_platform::get().workers;
        workers.for_each_chunk(&mut jobs, 1, |_, job| {
            let mut scratch = Scratch::default();
            for (voxels, spans, _) in job {
                for &span in spans.iter() {
                    voxels.clear_soil(span);
                    voxels.add_soil(span, terrain, &mut scratch);
                }
            }
        });
    }

    pub(super) fn bricks(&self) -> usize {
        let across = self.dims / PIECE;
        (across.x * across.z) as usize
    }

    /// The brick a span within one (a piece) lies in.
    pub(super) fn brick_of(&self, span: Span) -> usize {
        let across = self.dims / PIECE;
        let r = span.0.rem_euclid(self.dims) / PIECE;
        (r.x + across.x * r.z) as usize
    }

    /// A job for each brick with work in `by_brick`: its marks, for a
    /// worker to mark apart from the others, the work, and whether it
    /// was done.
    pub(super) fn jobs<T>(&mut self, by_brick: Vec<Vec<T>>) -> Vec<(Voxels<'_>, Vec<T>, bool)> {
        let dims = self.dims;
        let size = (PIECE * PIECE * dims.y) as usize;
        let area = (PIECE * PIECE) as usize;
        let bricks = (self.counts.chunks_mut(size))
            .zip(self.bits.chunks_mut(size / 32))
            .zip(self.layers.chunks_mut(area))
            .zip(self.soils.chunks_mut(size / 4))
            .zip(self.soil_layers.chunks_mut(area));
        (bricks.enumerate())
            .zip(by_brick)
            .filter(|(_, work)| !work.is_empty())
            .map(
                |((k, ((((counts, bits), layers), soils), soil_layers)), work)| {
                    let voxels = Voxels {
                        dims,
                        base: (k * size, k * area),
                        counts,
                        bits,
                        layers,
                        soils,
                        soil_layers,
                    };
                    (voxels, work, false)
                },
            )
            .collect()
    }
}

/// Some of a grid's marks, whole bricks of them (all, or one brick, for a
/// worker to mark apart from the others): those from slot and column
/// `base` on.
#[derive(Debug)]
pub(super) struct Voxels<'a> {
    pub(super) dims: IVec3,
    pub(super) base: (usize, usize),
    pub(super) counts: &'a mut [u32],
    pub(super) bits: &'a mut [u32],
    pub(super) layers: &'a mut [u64],
    pub(super) soils: &'a mut [u32],
    pub(super) soil_layers: &'a mut [u64],
}

/// A worker's buffers for reading the terrain's voxels, kept from one span
/// to the next.
#[derive(Debug, Default)]
pub(super) struct Scratch {
    pub(super) voxels: Vec<rook_terrain::Voxel>,
    pub(super) soils: Vec<u8>,
}

impl Voxels<'_> {
    /// Mark nothing in the voxels of `span`.
    pub(super) fn clear(&mut self, span: Span) {
        let base = self.base;
        let Self {
            dims,
            counts,
            bits,
            layers,
            soils,
            soil_layers,
            ..
        } = self;
        each(*dims, span, |_, slot| {
            let i = slot.index - base.0;
            counts[i] = 0;
            bits[i / 32] &= !(1 << (i % 32));
            layers[slot.column - base.1] &= !(1 << slot.layer);
            soils[i / 4] &= !(0xff << ((i % 4) * 8));
            soil_layers[slot.column - base.1] &= !(1 << slot.layer);
        });
    }

    /// Add the blockers (standing in their columns) and the terrain to the
    /// voxels of `span`.
    pub(super) fn take(&mut self, span: Span, from: Sources<'_>, scratch: &mut Scratch) {
        for i in from.columns.near(span, from.blockers) {
            if let Some(b) = &from.blockers[i] {
                self.apply(b, true, span);
            }
        }
        self.add_soil(span, from.terrain, scratch);
    }

    /// Forget the terrain in the voxels of `span`.
    pub(super) fn clear_soil(&mut self, span: Span) {
        let base = self.base;
        let Self {
            dims,
            soils,
            soil_layers,
            ..
        } = self;
        each(*dims, span, |_, slot| {
            let i = slot.index - base.0;
            soils[i / 4] &= !(0xff << ((i % 4) * 8));
            soil_layers[slot.column - base.1] &= !(1 << slot.layer);
        });
    }

    /// Mark how full of terrain each voxel of `span` is, as `terrain`
    /// says now, into voxels cleared of it. Most of the world holds none,
    /// and costs only a look at its chunks.
    pub(super) fn add_soil(&mut self, (lo, hi): Span, terrain: &Terrain, scratch: &mut Scratch) {
        terrain.soils((lo, hi), &mut scratch.voxels, &mut scratch.soils);
        let read = &scratch.soils;
        if read.is_empty() {
            return;
        }
        let size = hi - lo + 1;
        let base = self.base;
        let Self {
            dims,
            soils,
            soil_layers,
            ..
        } = self;
        each(*dims, (lo, hi), |k, slot| {
            // As `Terrain::soils` reads them: X fastest, then Y, then Z.
            let r = k - lo;
            let soil = read[((r.z * size.y + r.y) * size.x + r.x) as usize];
            if soil != 0 {
                let i = slot.index - base.0;
                soils[i / 4] |= u32::from(soil) << ((i % 4) * 8);
                soil_layers[slot.column - base.1] |= 1 << slot.layer;
            }
        });
    }

    /// Count `b` in (`add`) or out of the voxels of `span` whose centers
    /// it holds.
    pub(super) fn apply(&mut self, b: &Blocker, add: bool, span: Span) {
        let Some(within) = clip(b.centers(), span) else {
            return;
        };
        let to_local = b.axes.transpose();
        let base = self.base;
        let Self {
            dims,
            counts,
            bits,
            layers,
            ..
        } = self;
        each(*dims, within, |k, slot| {
            let local = to_local * (center(k) - b.center);
            if !local.abs().cmple(b.half).all() {
                return;
            }
            let i = slot.index - base.0;
            let column = slot.column - base.1;
            let (word, bit) = (i / 32, 1 << (i % 32));
            let layer = 1 << slot.layer;
            if add {
                counts[i] += 1;
                bits[word] |= bit;
                layers[column] |= layer;
            } else {
                // Saturating: a voxel still to mark holds another place's
                // counts until it is.
                counts[i] = counts[i].saturating_sub(1);
                if counts[i] == 0 {
                    bits[word] &= !bit;
                    layers[column] &= !layer;
                }
            }
        });
    }
}
