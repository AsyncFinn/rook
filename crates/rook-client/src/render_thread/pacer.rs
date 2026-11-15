//! When each frame starts: one after the other or overlapped with the last,
//! whichever keeps frames coming fastest without adding latency.

use std::time::{Duration, Instant};

/// How the frames run: one after the other, or overlapped, and when the
/// main thread starts each.
///
/// Overlap isn't free: the two sides contend for cores, and under a cap
/// that serial frames keep to it only adds latency
/// (docs/behavior/rendering/frame-threads.md#rooks-frames-measured). So
/// both ways are measured and chosen per frame, Unreal's
/// `r.OneFrameThreadLag` decided by cost: serially while that keeps to the
/// cap or refresh (within a tenth), else whichever way gives shorter
/// frames, serially unless overlap is a tenth faster. The way not in use
/// is probed every so often (two frames, the first a transition), each
/// lost probe waiting twice as long for the next.
///
/// Overlapped, frames start just in time, as NVIDIA Reflex and AMD
/// Anti-Lag start them ([`Pacer::hand_over`]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pacer {
    /// How long after a frame is handed over the next one starts.
    delay: Duration,
    /// Whether the frame now running runs serially.
    serial: bool,
    /// What a frame costs each way, smoothed while frames run that way and
    /// set by probes.
    serial_cost: Option<Duration>,
    overlap_cost: Option<Duration>,
    /// Frames in a row that wanted the other way.
    against: u32,
    /// Frames left in a probe of the other way.
    probing: u32,
    /// How long frames have run since the last probe, and how long they
    /// run before the next.
    since_probe: Duration,
    probe_wait: Duration,
}

/// The longest [`Pacer::delay`]: longer than any frame, so one stall
/// can't hold the frames after it back for long.
const MAX_DELAY: Duration = Duration::from_millis(100);

/// Frames in a row that want the other way before frames switch: one is
/// a hitch, two are a trend.
const AGAINST: u32 = 2;

/// Frames a probe runs; only the last is measured.
pub(super) const PROBE: u32 = 2;

/// How long frames run before probing the other way, at first and at
/// most.
const MIN_PROBE_WAIT: Duration = Duration::from_secs(1);
pub(super) const MAX_PROBE_WAIT: Duration = Duration::from_secs(8);

impl Default for Pacer {
    fn default() -> Self {
        Self {
            delay: Duration::ZERO,
            serial: true,
            serial_cost: None,
            overlap_cost: None,
            against: 0,
            probing: 0,
            since_probe: Duration::ZERO,
            probe_wait: MIN_PROBE_WAIT,
        }
    }
}

impl Pacer {
    /// Whether the frame being handed over runs serially: the main thread
    /// waits for it before starting the next.
    pub(crate) fn serial(&self) -> bool {
        self.serial
    }

    /// When to start the frame after the one handed over at `sent`.
    pub(crate) fn start_at(&self, sent: Option<Instant>) -> Option<Instant> {
        sent.map(|sent| sent + self.delay)
    }

    /// An overlapped frame's hand-over that `waited` for the render
    /// thread, which had sat `idle` before it (one of them is zero). A
    /// wait starts the next frame later by half of it, idling earlier by a
    /// quarter: jitter costs a little frame rate rather than latency.
    pub(crate) fn hand_over(&mut self, waited: Duration, idle: Duration) {
        self.delay = if waited > idle {
            (self.delay + waited / 2).min(MAX_DELAY)
        } else {
            self.delay.saturating_sub(idle / 4)
        };
    }

    /// A frame run the way frames now run cost `cost`, to keep to
    /// `period` between frames (`None`: as fast as they come).
    pub(crate) fn finished(&mut self, cost: Duration, period: Option<Duration>) {
        let measured = if self.serial {
            &mut self.serial_cost
        } else {
            &mut self.overlap_cost
        };
        if self.probing > 0 {
            self.probing -= 1;
            if self.probing == 0 {
                *measured = Some(cost);
                self.since_probe = Duration::ZERO;
                if self.wants_serial(period) == self.serial {
                    self.probe_wait = MIN_PROBE_WAIT;
                } else {
                    self.probe_wait = (self.probe_wait * 2).min(MAX_PROBE_WAIT);
                    self.switch();
                }
            }
            return;
        }
        *measured = Some(measured.map_or(cost, |m| (m * 3 + cost) / 4));
        if self.wants_serial(period) != self.serial {
            self.against += 1;
            if self.against >= AGAINST {
                self.switch();
                self.since_probe = Duration::ZERO;
            }
            return;
        }
        self.against = 0;
        if self.serial && self.serial_fits(period) {
            return;
        }
        self.since_probe += period.unwrap_or_default().max(cost);
        if self.since_probe >= self.probe_wait {
            self.switch();
            self.probing = PROBE;
        }
    }

    /// Whether serial frames, as last measured, keep to `period`, or come
    /// within a tenth of it: a frame a little late under a cap costs less
    /// than a second of older input.
    fn serial_fits(&self, period: Option<Duration>) -> bool {
        matches!((self.serial_cost, period), (Some(cost), Some(period)) if cost * 10 <= period * 11)
    }

    /// Which way frames should run, by what each way last cost. A way not
    /// yet measured is tried.
    fn wants_serial(&self, period: Option<Duration>) -> bool {
        let Some(serial) = self.serial_cost else {
            return true;
        };
        if self.serial_fits(period) {
            return true;
        }
        let Some(overlap) = self.overlap_cost else {
            return false;
        };
        let overlap_fits = period.is_some_and(|period| overlap <= period);
        !(overlap_fits || overlap * 10 < serial * 9)
    }

    fn switch(&mut self) {
        self.serial = !self.serial;
        self.against = 0;
        self.delay = Duration::ZERO;
    }
}
