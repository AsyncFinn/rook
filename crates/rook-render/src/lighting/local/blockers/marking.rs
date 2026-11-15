//! Marking the voxels for a frame and handing the GPU what changed.

use super::*;

impl Marked<'_> {
    /// The voxel slots (`Grid`'s) holding what changed, as runs of whole
    /// layers of each piece's brick, sorted and joined: what goes up to
    /// the GPU.
    pub(in crate::lighting::local) fn slot_runs(&self) -> Vec<Range<usize>> {
        let d = self.dims;
        let Touched::Spans(spans) = &self.touched else {
            let all = 0..(d.x * d.y * d.z) as usize;
            return vec![all];
        };
        let layer = (PIECE * PIECE) as usize;
        let mut runs = Vec::new();
        for &(lo, hi) in spans {
            // The layers, as at most two runs where y wraps around the slots.
            let (y0, y1) = (lo.y.rem_euclid(d.y), hi.y.rem_euclid(d.y));
            let layers = if hi.y - lo.y + 1 >= d.y {
                vec![(0, d.y - 1)]
            } else if y0 <= y1 {
                vec![(y0, y1)]
            } else {
                vec![(y0, d.y - 1), (0, y1)]
            };
            for piece in pieces((lo, hi)) {
                // The brick's first slot.
                let r = piece.0.rem_euclid(d);
                let start = slot_of(d, IVec3::new(r.x - r.x % PIECE, 0, r.z - r.z % PIECE));
                for &(a, b) in &layers {
                    runs.push(start + layer * a as usize..start + layer * (b + 1) as usize);
                }
            }
        }
        runs.sort_by_key(|r| r.start);
        let mut joined: Vec<Range<usize>> = Vec::with_capacity(runs.len());
        for run in runs {
            match joined.last_mut() {
                Some(last) if run.start <= last.end => last.end = last.end.max(run.end),
                _ => joined.push(run),
            }
        }
        joined
    }
}

impl Blockers {
    /// Whether the voxels should be marked again for parts or terrain that
    /// changed.
    pub(in crate::lighting::local) fn due(&self) -> bool {
        self.changed
    }

    /// One bit per voxel of the volume starting at voxel `low`, set where
    /// a voxel's center is inside a blocker, and a byte per voxel, how full
    /// of terrain it is, all kept at [`Grid`]'s slots, with the voxels
    /// marked again since the last mark; `None` while the voxels a moved
    /// volume came onto (or all of them, after more changes than parts)
    /// are still being marked, within
    /// [`BUDGET`] a frame, the last volume's staying as they were. A volume
    /// of the same size only takes the blockers that changed since and the
    /// voxels it moved onto; the first is marked whole at once.
    pub(in crate::lighting::local) fn mark(
        &mut self,
        low: IVec3,
        dims: IVec3,
    ) -> Option<Marked<'_>> {
        profiling::scope!("Blockers::mark");
        if (self.grid.as_ref()).is_none_or(|grid| grid.low != low || grid.dims != dims) {
            self.release_held();
        }
        match &mut self.grid {
            Some(grid) if grid.dims == dims => {
                // The voxels it came onto change; those it still covers
                // keep their slots, so neither they nor what the fill made
                // of them go up again.
                if grid.low != low {
                    for piece in grid.move_to(low) {
                        self.touched.add(piece);
                    }
                }
                // Taken everywhere at once, pieces still to mark included:
                // those are cleared and take the blockers as they are then.
                let span = grid.span();
                let mut moves = Vec::new();
                for (before, after) in std::mem::take(&mut self.moved) {
                    // Voxels both boxes fill whole are the same after: a
                    // big part that moves a little marks only its edges.
                    let kept = before
                        .zip(after)
                        .and_then(|(a, b)| clip(a.whole()?, b.whole()?));
                    for (b, add) in [(before, false), (after, true)] {
                        let Some(b) = b else {
                            continue;
                        };
                        let Some(within) = clip(b.centers(), span) else {
                            continue;
                        };
                        for piece in minus(within, kept) {
                            moves.push((b, add, piece));
                            self.touched.add(piece);
                        }
                    }
                }
                grid.apply_moves(&moves);
                let soils: Vec<Span> = (self.terrain.take_edits().into_iter())
                    .filter_map(|edit| clip(edit, span))
                    .collect();
                grid.apply_soils(&soils, &self.terrain);
                for edit in soils {
                    self.touched.add(edit);
                }
                let from = Sources {
                    blockers: &self.blockers,
                    columns: &self.columns,
                    terrain: &self.terrain,
                };
                if !grid.work(BUDGET, from) {
                    return None;
                }
            }
            grid => {
                self.moved.clear();
                // A new grid reads all of the terrain as it is.
                self.terrain.take_edits();
                let from = Sources {
                    blockers: &self.blockers,
                    columns: &self.columns,
                    terrain: &self.terrain,
                };
                *grid = Some(Grid::new(low, dims, from));
                self.touched = Touched::All;
            }
        }
        self.changed = false;
        let grid = self.grid.as_ref().expect("just marked");
        Some(Marked {
            bits: &grid.bits,
            layers: &grid.layers,
            soils: &grid.soils,
            soil_layers: &grid.soil_layers,
            touched: std::mem::take(&mut self.touched),
            dims,
        })
    }
}
