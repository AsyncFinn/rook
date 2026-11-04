//! The render thread: pipelined rendering, as Bevy's pipelined rendering
//! and Unreal's render thread run it, and as Roblox's render thread does
//! (docs/behavior/rendering/frame-threads.md). The main thread steps the
//! next frame while this thread records, submits and presents the last
//! one, so a frame costs about its slower half rather than the sum.
//!
//! What the renderer needs lives in one value (`T`) that is handed to the
//! thread with each frame's job and handed back when the job is done, as
//! Bevy sends its render app across. So nothing is shared behind a lock:
//! the main thread can only touch the renderer between frames, at the
//! hand-over ([`RenderThread::get`]), where the scene's changes go in
//! (rook-view's `ToRender`, pipelined rendering's extract).
//! docs/design/architecture.md#frame-order has the frame's order.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// A frame's work on the render thread; returns how long of it was spent
/// waiting for the display (for a swap chain image), which isn't cost.
type Job<T> = Box<dyn FnOnce(&mut T) -> Duration + Send>;

/// A job done: when, and how long it waited for the display.
type Done<T> = (T, Instant, Duration);

pub(crate) struct RenderThread<T: Send + 'static> {
    /// The renderer's side, while no frame is in flight.
    home: Option<T>,
    jobs: Option<Sender<(T, Job<T>)>>,
    done: Receiver<Done<T>>,
    thread: Option<JoinHandle<()>>,
    /// When the frame in flight was handed over.
    sent: Option<Instant>,
    /// When the last frame was handed over, and whether it overlapped
    /// this one.
    last_sent: Option<Instant>,
    overlapped: bool,
    /// When this frame started (took its input), how long its start was
    /// held past when the pacer would have started it (by a frame rate
    /// cap, or the window's events), and the time between frames it should
    /// keep to ([`RenderThread::hand_over`]).
    started: Option<Instant>,
    held: Duration,
    /// How long this frame's hand-over waited for the frame before: not
    /// its own cost.
    waited: Duration,
    period: Option<Duration>,
    pacer: Pacer,
}

/// A frame finished on the render thread.
struct Finished {
    at: Instant,
    /// Its cost there: from hand-over to done, less any wait for the display.
    render: Duration,
}

impl<T: Send + 'static> RenderThread<T> {
    /// Start the thread, `target` on this side until the first frame.
    pub(crate) fn new(target: T) -> Self {
        let (jobs, inbox) = channel::<(T, Job<T>)>();
        let (finished, done) = channel();
        let thread = std::thread::Builder::new()
            .name("rook render".into())
            .spawn(move || {
                for (mut target, job) in inbox {
                    let display = job(&mut target);
                    if finished.send((target, Instant::now(), display)).is_err() {
                        return;
                    }
                }
            })
            .expect("couldn't start the render thread");
        Self {
            home: Some(target),
            jobs: Some(jobs),
            done,
            thread: Some(thread),
            sent: None,
            last_sent: None,
            overlapped: false,
            started: None,
            held: Duration::ZERO,
            waited: Duration::ZERO,
            period: None,
            pacer: Pacer::default(),
        }
    }

    /// The renderer's side, waiting for the frame in flight to finish
    /// first (for what can't wait for the next frame: a resize, a new
    /// present mode).
    pub(crate) fn get(&mut self) -> &mut T {
        self.receive();
        self.home.as_mut().expect("home once the frame finished")
    }

    /// The renderer's side at the hand-over of a frame that `started`
    /// (took its input) then, waiting for the frame in flight to finish
    /// first. `period` is the time between frames to keep to (a frame
    /// rate cap's or the display's), `None` for as fast as they come. How
    /// each frame went decides how the next run ([`Pacer`]).
    pub(crate) fn hand_over(&mut self, started: Instant, period: Option<Duration>) -> &mut T {
        let due = self.pacer.start_at(self.last_sent);
        self.held = due.map_or(Duration::ZERO, |due| started.saturating_duration_since(due));
        self.started = Some(started);
        self.period = period;
        let at = Instant::now();
        self.waited = Duration::ZERO;
        if let Some(finished) = self.receive() {
            self.waited = at.elapsed();
            let idle = at.saturating_duration_since(finished.at);
            self.pacer.hand_over(self.waited, idle);
        }
        self.home.as_mut().expect("home once the frame finished")
    }

    /// Take the renderer's side back from the thread when it's there;
    /// the frame it finished.
    fn receive(&mut self) -> Option<Finished> {
        if self.home.is_some() {
            return None;
        }
        profiling::scope!("wait for the render thread");
        match self.done.recv() {
            Ok((target, at, display)) => {
                self.home = Some(target);
                let sent = self.sent.take()?;
                let render = at.saturating_duration_since(sent).saturating_sub(display);
                Some(Finished { at, render })
            }
            // The thread panicked: panic here with its message.
            Err(_) => match self.thread.take().map(JoinHandle::join) {
                Some(Err(panic)) => std::panic::resume_unwind(panic),
                _ => panic!("the render thread stopped"),
            },
        }
    }

    /// Run `job` on the render thread with the renderer's side, which
    /// stays there until the next frame's [`RenderThread::hand_over`]
    /// takes it back, or [`RenderThread::get`]. A frame the [`Pacer`]
    /// runs serially is waited for here instead, so the next frame takes
    /// its input after this one is shown. `job` returns how long it
    /// waited for the display.
    pub(crate) fn send(&mut self, job: impl FnOnce(&mut T) -> Duration + Send + 'static) {
        self.get();
        let target = self.home.take().expect("got above");
        let sent = Instant::now();
        // Overlapped, a frame costs the time from one hand-over to the
        // next, less any time its start was held for other reasons.
        if self.overlapped
            && let Some(last) = self.last_sent
        {
            let cost = sent
                .saturating_duration_since(last)
                .saturating_sub(self.held);
            self.pacer.finished(cost, self.period);
        }
        self.sent = Some(sent);
        self.last_sent = Some(sent);
        self.overlapped = !self.pacer.serial();
        let jobs = self.jobs.as_ref().expect("open until dropped");
        if jobs.send((target, Box::new(job))).is_err() {
            // Only a panic on the thread closes it; `get` raises it.
            self.get();
        }
        // Serially, its step plus its render.
        if !self.overlapped
            && let Some(finished) = self.receive()
        {
            let took = self
                .started
                .map_or(Duration::ZERO, |s| sent.saturating_duration_since(s));
            let main = took.saturating_sub(self.waited);
            self.pacer.finished(main + finished.render, self.period);
        }
    }

    /// When the main thread should start its next frame ([`Pacer`]);
    /// `None` with no frame in flight: start now.
    pub(crate) fn start_next_at(&self) -> Option<Instant> {
        self.pacer.start_at(self.sent)
    }
}

impl<T: Send + 'static> Drop for RenderThread<T> {
    /// Take the renderer's side back, so it's dropped here (a window's
    /// surface with its window), then stop the thread.
    fn drop(&mut self) {
        if self.home.is_none()
            && self.thread.is_some()
            && let Ok((target, ..)) = self.done.recv()
        {
            self.home = Some(target);
        }
        self.jobs = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

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
const PROBE: u32 = 2;

/// How long frames run before probing the other way, at first and at
/// most.
const MIN_PROBE_WAIT: Duration = Duration::from_secs(1);
const MAX_PROBE_WAIT: Duration = Duration::from_secs(8);

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

#[cfg(test)]
mod tests;
