//! The screen's GUI kept per piece (docs/design/architecture.md#redrawing-what-changed):
//! each piece packed once, written to the quad buffer alone when the
//! pieces around it keep their sizes, and the draw runs joined across
//! pieces.

use std::sync::Arc;

use rook_types::GuiQuad;

use super::images::Run;
use super::{Held, Packed, Target};

/// A piece as last given, and what was made of it.
#[derive(Debug)]
struct Piece {
    source: Arc<[GuiQuad]>,
    packed: Packed,
    held: Held,
    runs: Vec<Run>,
}

#[derive(Debug, Default)]
pub(super) struct ScreenPieces {
    /// The list last given.
    list: Option<Arc<[Arc<[GuiQuad]>]>>,
    pieces: Vec<Piece>,
}

impl ScreenPieces {
    /// Whether `list` is the one last given.
    pub(super) fn is(&self, list: &Arc<[Arc<[GuiQuad]>]>) -> bool {
        self.list.as_ref().is_some_and(|l| Arc::ptr_eq(l, list))
    }

    /// For each piece of `list`, where it was last time (`None`: new).
    pub(super) fn kept(&self, list: &[Arc<[GuiQuad]>]) -> Vec<Option<usize>> {
        let at: rustc_hash::FxHashMap<*const GuiQuad, usize> = (self.pieces.iter().enumerate())
            .map(|(i, p)| (p.source.as_ptr(), i))
            .collect();
        list.iter().map(|p| at.get(&p.as_ptr()).copied()).collect()
    }

    /// The groups the kept pieces hold.
    pub(super) fn held<'a>(&'a self, kept: &'a [Option<usize>]) -> impl Iterator<Item = &'a Held> {
        kept.iter().flatten().map(|&i| &self.pieces[i].held)
    }

    /// Draw everything again next time.
    pub(super) fn forget(&mut self) {
        *self = Self::default();
    }

    /// Show `list` in `target` from now on: its kept pieces as they were,
    /// `fresh` (packed, in order) for the new ones.
    pub(super) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &mut Target,
        list: &Arc<[Arc<[GuiQuad]>]>,
        kept: &[Option<usize>],
        fresh: Vec<(Packed, Held)>,
    ) {
        let mut old: Vec<Option<Piece>> = std::mem::take(&mut self.pieces)
            .into_iter()
            .map(Some)
            .collect();
        let old_counts: Vec<usize> = old.iter().flatten().map(|p| p.packed.len()).collect();
        let mut fresh = fresh.into_iter();
        let pieces: Vec<Piece> = (list.iter().zip(kept))
            .map(|(source, k)| match k.and_then(|i| old[i].take()) {
                Some(piece) => piece,
                None => {
                    let (packed, held) = fresh.next().expect("one per new piece");
                    let runs = super::images::runs(packed.iter().map(|(_, image)| *image));
                    Piece {
                        source: source.clone(),
                        packed,
                        held,
                        runs,
                    }
                }
            })
            .collect();
        // Pieces keeping their sizes and places: write the new ones alone.
        let in_place = pieces.len() == old_counts.len()
            && (pieces.iter().zip(&old_counts)).all(|(p, &n)| p.packed.len() == n);
        if in_place && target.quads.is_some() {
            let mut at = 0;
            for (i, p) in pieces.iter().enumerate() {
                if kept[i] != Some(i) {
                    target.write(queue, at, &p.packed);
                }
                at += p.packed.len() as u32;
            }
        } else {
            let all: Packed = pieces
                .iter()
                .flat_map(|p| p.packed.iter().copied())
                .collect();
            target.set(device, queue, &all);
        }
        target.runs = joined(&pieces);
        self.pieces = pieces;
        self.list = Some(list.clone());
    }
}

/// The pieces' runs, one after another, a run spanning pieces when they
/// draw the same image.
fn joined(pieces: &[Piece]) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let mut at = 0;
    for p in pieces {
        for run in &p.runs {
            let quads = run.quads.start + at..run.quads.end + at;
            match out.last_mut() {
                Some(last) if last.image == run.image && last.quads.end == quads.start => {
                    last.quads.end = quads.end;
                }
                _ => out.push(Run {
                    quads,
                    image: run.image,
                }),
            }
        }
        at += p.packed.len() as u32;
    }
    out
}
