//! Parts to draw again because a file they waited for arrived, a frame's
//! budget at a time. One file can be waited for by tens of thousands of
//! parts (a texture shared across a place); drawing them all in the frame
//! it lands stalled that frame for most of a second, and the renderer's
//! side of it (the hand-over, the instance upload) with it. So arrivals
//! queue, nearest the camera first, and each frame draws them until its
//! budget is spent: the time-sliced streaming that Unreal's
//! `s.AsyncLoadingTimeLimit` and Unity's `asyncUploadTimeSlice` bound
//! (docs/design/architecture.md#per-frame-budgets). A part still queued
//! draws what it drew while its file loaded, as if the file had come a
//! few frames later, which a download can always do.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use rook_dom::InstanceId;
use rustc_hash::FxHashSet;

/// What a frame spends drawing parts whose files arrived (at least one
/// part, so the queue always moves).
pub(crate) const ARRIVALS_BUDGET: Duration = Duration::from_millis(2);

#[derive(Debug, Default)]
pub(crate) struct Arrivals {
    queue: VecDeque<InstanceId>,
    queued: FxHashSet<InstanceId>,
}

impl Arrivals {
    pub(crate) fn push(&mut self, part: InstanceId) {
        if self.queued.insert(part) {
            self.queue.push_back(part);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Hand queued parts to `each` in order until `budget` is spent (at
    /// least one), or all of them without one.
    pub(crate) fn drain(&mut self, budget: Option<Duration>, mut each: impl FnMut(InstanceId)) {
        let start = Instant::now();
        while let Some(part) = self.queue.pop_front() {
            self.queued.remove(&part);
            each(part);
            if budget.is_some_and(|b| start.elapsed() >= b) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: u32) -> Vec<InstanceId> {
        let mut dom = rook_dom::Dom::new();
        (0..n).map(|_| dom.create_unknown("Part")).collect()
    }

    /// A budget spent by the first part still lets one through, and the
    /// rest wait for the next frame in order; without a budget all go.
    #[test]
    fn arrivals_go_a_budget_at_a_time_in_order() {
        let parts = ids(4);
        let mut arrivals = Arrivals::default();
        for &p in parts.iter().chain(&parts[..2]) {
            arrivals.push(p);
        }
        let mut drawn = Vec::new();
        arrivals.drain(Some(Duration::ZERO), |p| drawn.push(p));
        assert_eq!(drawn, parts[..1]);
        arrivals.drain(None, |p| drawn.push(p));
        assert_eq!(drawn, parts);
        assert!(arrivals.is_empty());
    }
}
