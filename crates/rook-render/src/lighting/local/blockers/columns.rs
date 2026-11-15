//! Where blockers stand, by columns of voxels, so a change finds the
//! blockers near it.

use super::*;

/// A column's width, in voxels.
pub(super) const COLUMN: i32 = 16;

/// Blockers across more columns than this along an axis (a baseplate) are
/// kept apart and tried for every span, rather than listed in thousands of
/// columns.
pub(super) const WIDE: i32 = 16;

/// Which blockers stand in each 64-stud column of the world, so the voxels
/// a moving volume comes onto take only the blockers there, not every part
/// in the place.
#[derive(Debug, Default)]
pub(super) struct Columns {
    /// Indices into the blockers, by column.
    pub(super) columns: FxHashMap<IVec2, Vec<u32>>,
    pub(super) wide: Vec<u32>,
}

/// The columns `span` reaches.
pub(super) fn columns_of((lo, hi): Span) -> (IVec2, IVec2) {
    let column = |v: IVec3| IVec2::new(v.x, v.z).div_euclid(IVec2::splat(COLUMN));
    (column(lo), column(hi))
}

pub(super) fn is_wide((lo, hi): (IVec2, IVec2)) -> bool {
    (hi - lo).cmpge(IVec2::splat(WIDE)).any()
}

impl Columns {
    pub(super) fn clear(&mut self) {
        self.columns.clear();
        self.wide.clear();
    }

    pub(super) fn insert(&mut self, i: usize, b: &Blocker) {
        let range = columns_of(b.centers());
        if is_wide(range) {
            self.wide.push(i as u32);
            return;
        }
        for c in cells(range) {
            self.columns.entry(c).or_default().push(i as u32);
        }
    }

    pub(super) fn remove(&mut self, i: usize, b: &Blocker) {
        let take = |list: &mut Vec<u32>| {
            if let Some(at) = list.iter().position(|&j| j as usize == i) {
                list.swap_remove(at);
            }
        };
        let range = columns_of(b.centers());
        if is_wide(range) {
            take(&mut self.wide);
            return;
        }
        for c in cells(range) {
            if let Some(list) = self.columns.get_mut(&c) {
                take(list);
                if list.is_empty() {
                    self.columns.remove(&c);
                }
            }
        }
    }

    /// The blockers that may reach `span`, each once.
    pub(super) fn near(&self, span: Span, blockers: &[Option<Blocker>]) -> Vec<usize> {
        let (q0, q1) = columns_of(span);
        let mut near: Vec<usize> = self.wide.iter().map(|&i| i as usize).collect();
        for c in cells((q0, q1)) {
            for &i in self.columns.get(&c).into_iter().flatten() {
                let Some(b) = &blockers[i as usize] else {
                    continue;
                };
                // A blocker over several columns is taken in the first of
                // them the span reaches.
                if columns_of(b.centers()).0.max(q0) == c {
                    near.push(i as usize);
                }
            }
        }
        near
    }
}

/// Each column from the first corner to the second.
pub(super) fn cells((lo, hi): (IVec2, IVec2)) -> impl Iterator<Item = IVec2> {
    (lo.y..=hi.y).flat_map(move |z| (lo.x..=hi.x).map(move |x| IVec2::new(x, z)))
}
