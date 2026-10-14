//! Thread scheduling and frame timing.
//!
//! The scheduler owns the DataModel's clock and its queues of pending work:
//! timers (legacy `wait`/`delay`/`spawn` and `task.wait`/`task.delay`) and
//! the deferred queue (`task.defer`, Deferred signal handlers). It doesn't
//! run anything itself. The engine drives the frame and asks for the work
//! due at each resumption point. Jobs are an opaque `T` (for the Lua layer,
//! a thread to resume or a function to call).
//!
//! Frame order and resumption points: `docs/design/architecture.md` §6.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use std::time::Instant;

/// Which resumption point a timer belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Queue {
    /// `wait`, `delay`, `spawn`: resumed by the legacy step, before simulation.
    Legacy,
    /// `task.wait`, `task.delay`: resumed after Heartbeat.
    Task,
}

/// Shortest legacy `wait` and the default when no time is given. The
/// creator-docs signature gives `wait(seconds: number = 0.03)`; whether
/// smaller values round up to it is Inference.
pub const LEGACY_MIN_WAIT: f64 = 0.03;

/// Timers due within this many seconds of the current time count as due,
/// so fixed-step float rounding never delays one by a whole frame.
const EPSILON: f64 = 1e-9;

/// Where time comes from.
#[derive(Debug, Clone, Copy)]
pub enum Clock {
    /// Each frame advances by exactly `step` seconds (headless runs, tests,
    /// conformance), so results are reproducible.
    Fixed { step: f64 },
    /// Wall-clock time.
    Real { start: Instant },
}

#[derive(Debug)]
struct Timer<T> {
    at: f64,
    seq: u64,
    queue: Queue,
    job: T,
}

impl<T> PartialEq for Timer<T> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl<T> Eq for Timer<T> {}
impl<T> PartialOrd for Timer<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Timer<T> {
    /// Reversed so `BinaryHeap` pops the earliest timer first.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .at
            .total_cmp(&self.at)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

#[derive(Debug)]
pub struct Scheduler<T> {
    clock: Clock,
    now: f64,
    delta: f64,
    frame: u64,
    seq: u64,
    legacy: BinaryHeap<Timer<T>>,
    task: BinaryHeap<Timer<T>>,
    deferred: VecDeque<T>,
}

impl<T> Scheduler<T> {
    pub fn new(clock: Clock) -> Self {
        Self {
            clock,
            now: 0.0,
            delta: 0.0,
            frame: 0,
            seq: 0,
            legacy: BinaryHeap::new(),
            task: BinaryHeap::new(),
            deferred: VecDeque::new(),
        }
    }

    /// A fixed-step clock at `hz` frames per second.
    pub fn fixed(hz: f64) -> Self {
        Self::new(Clock::Fixed { step: 1.0 / hz })
    }

    pub fn real() -> Self {
        Self::new(Clock::Real {
            start: Instant::now(),
        })
    }

    /// Seconds since the DataModel started (`time()`, `workspace.DistributedGameTime`).
    pub fn now(&self) -> f64 {
        self.now
    }

    /// Length of the current frame in seconds.
    pub fn delta(&self) -> f64 {
        self.delta
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Start a new frame, advancing the clock. Returns the frame's delta.
    pub fn begin_frame(&mut self) -> f64 {
        let next = match self.clock {
            Clock::Fixed { step } => self.now + step,
            Clock::Real { start } => start.elapsed().as_secs_f64(),
        };
        self.delta = next - self.now;
        self.now = next;
        self.frame += 1;
        self.delta
    }

    /// Fixed clocks only: jump forward whole frames so that the next frame
    /// is the first one at or after the earliest timer. Lets headless runs
    /// skip idle stretches. Returns whether anything is pending.
    pub fn skip_idle(&mut self) -> bool {
        let Clock::Fixed { step } = self.clock else {
            return self.has_work();
        };
        if !self.deferred.is_empty() {
            return true;
        }
        let Some(next) = self.next_timer() else {
            return false;
        };
        let frames = ((next - self.now) / step - EPSILON).ceil() - 1.0;
        if frames > 0.0 {
            self.now += frames * step;
        }
        true
    }

    fn next_timer(&self) -> Option<f64> {
        let a = self.legacy.peek().map(|t| t.at);
        let b = self.task.peek().map(|t| t.at);
        match (a, b) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    pub fn has_work(&self) -> bool {
        !self.deferred.is_empty() || !self.legacy.is_empty() || !self.task.is_empty()
    }

    /// Run `job` once `delay` seconds have passed, at `queue`'s resumption point.
    pub fn schedule(&mut self, queue: Queue, delay: f64, job: T) {
        let at = self.now + delay.max(0.0);
        self.seq += 1;
        let timer = Timer {
            at,
            seq: self.seq,
            queue,
            job,
        };
        match queue {
            Queue::Legacy => self.legacy.push(timer),
            Queue::Task => self.task.push(timer),
        }
    }

    /// Jobs on `queue` whose time has come, earliest first; ties keep
    /// scheduling order.
    pub fn take_due(&mut self, queue: Queue) -> Vec<T> {
        let heap = match queue {
            Queue::Legacy => &mut self.legacy,
            Queue::Task => &mut self.task,
        };
        let mut out = Vec::new();
        while heap.peek().is_some_and(|t| t.at <= self.now + EPSILON) {
            let t = heap.pop().expect("peeked");
            debug_assert_eq!(t.queue, queue);
            out.push(t.job);
        }
        out
    }

    /// `task.defer` and Deferred signal handlers.
    pub fn defer(&mut self, job: T) {
        self.deferred.push_back(job);
    }

    /// Next deferred job. Jobs deferred while draining run in the same
    /// drain, after the ones already queued.
    pub fn pop_deferred(&mut self) -> Option<T> {
        self.deferred.pop_front()
    }

    /// Remove pending jobs matching `pred` (for `task.cancel`). Returns how
    /// many were removed.
    pub fn cancel(&mut self, mut pred: impl FnMut(&T) -> bool) -> usize {
        let before = self.legacy.len() + self.task.len() + self.deferred.len();
        self.legacy.retain(|t| !pred(&t.job));
        self.task.retain(|t| !pred(&t.job));
        self.deferred.retain(|j| !pred(j));
        before - (self.legacy.len() + self.task.len() + self.deferred.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timers_fire_in_time_order_then_schedule_order() {
        let mut s = Scheduler::fixed(60.0);
        s.schedule(Queue::Task, 0.04, "late");
        s.schedule(Queue::Task, 0.0, "a");
        s.schedule(Queue::Task, 0.0, "b");
        s.schedule(Queue::Legacy, 0.0, "legacy");
        s.begin_frame();
        assert_eq!(s.take_due(Queue::Task), ["a", "b"]);
        assert_eq!(s.take_due(Queue::Legacy), ["legacy"]);
        s.begin_frame();
        assert!(s.take_due(Queue::Task).is_empty());
        s.begin_frame();
        assert_eq!(s.take_due(Queue::Task), ["late"]);
    }

    #[test]
    fn skip_idle_lands_just_before_the_next_timer() {
        let mut s = Scheduler::fixed(60.0);
        s.schedule(Queue::Legacy, 10.0, ());
        assert!(s.skip_idle());
        s.begin_frame();
        assert_eq!(s.take_due(Queue::Legacy).len(), 1);
        assert!((s.now() - 10.0).abs() < 1e-6, "{}", s.now());
        assert!(!s.skip_idle());
    }

    #[test]
    fn deferred_is_fifo_and_cancellable() {
        let mut s = Scheduler::fixed(60.0);
        s.defer(1);
        s.defer(2);
        s.schedule(Queue::Task, 1.0, 3);
        assert_eq!(s.cancel(|j| *j == 2 || *j == 3), 2);
        assert_eq!(s.pop_deferred(), Some(1));
        assert_eq!(s.pop_deferred(), None);
    }
}
