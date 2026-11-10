//! `--bench`'s work counters (`rook_types::counters`): each counter's
//! median and largest frame, and the frames where a system touched a
//! large share of its collection while little in it changed, the sign of
//! a small change paying for the whole collection.

use rook_types::counters::{self, CHECKS, Counter, Counts, Kind};

use crate::stats::thousands;

/// A collection smaller than this is never flagged: rebuilding it whole
/// costs little.
const SMALL: u64 = 1_000;

/// Each frame's counts.
#[derive(Debug, Default)]
pub(crate) struct Frames(Vec<Counts>);

impl Frames {
    /// Drop what was counted before the frames start (loading).
    pub(crate) fn start() -> Self {
        counters::take();
        Self::default()
    }

    /// End a frame.
    pub(crate) fn frame(&mut self) {
        self.0.push(counters::take());
    }

    pub(crate) fn report(&self) {
        if !counters::ENABLED {
            eprintln!("work counters: not in this build (the `counters` feature)");
            return;
        }
        eprintln!("work counters per frame (median / max):");
        for &c in Counter::ALL {
            let mut values: Vec<u64> = self.0.iter().map(|f| f[c]).collect();
            if values.iter().all(|&v| v == 0) {
                continue;
            }
            values.sort_unstable();
            let median = values[values.len() / 2];
            let max = values[values.len() - 1];
            let size = if c.kind() == Kind::Size {
                " (size)"
            } else {
                ""
            };
            eprintln!(
                "  {}{size}: {} / {}",
                c.label(),
                thousands(median as usize),
                thousands(max as usize)
            );
        }
        for &(touched, changed, size) in CHECKS {
            let flagged: Vec<(usize, &Counts)> = (self.0.iter().enumerate())
                .filter(|(_, f)| whole_for_little(f[touched], f[changed], f[size]))
                .collect();
            let Some(&(first, f)) = flagged.first() else {
                continue;
            };
            eprintln!(
                "  FLAG {}: {} of {} frames touched a quarter or more of {} while {} changed (first: frame {first}, {} touched of {}, {} changed)",
                touched.label(),
                flagged.len(),
                self.0.len(),
                size.label(),
                changed.label(),
                thousands(f[touched] as usize),
                thousands(f[size] as usize),
                thousands(f[changed] as usize)
            );
        }
    }
}

/// Whether a frame touched a quarter or more of a collection that isn't
/// small while fewer than a tenth as many things changed.
fn whole_for_little(touched: u64, changed: u64, size: u64) -> bool {
    size >= SMALL && touched * 4 >= size && touched > changed * 10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_whole_work_for_little_change() {
        // One part changed, the whole scene rewritten.
        assert!(whole_for_little(90_000, 1, 90_000));
        // Many changed: the work answers them.
        assert!(!whole_for_little(90_000, 30_000, 90_000));
        // A small share of the collection.
        assert!(!whole_for_little(1_000, 1, 90_000));
        // A small collection.
        assert!(!whole_for_little(500, 0, 500));
    }
}
