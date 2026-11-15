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
//! (rook-scene's `ToRender`, pipelined rendering's extract).
//! docs/design/architecture/threads-and-frames.md#frame-order has the frame's order.

mod pacer;

pub(crate) use pacer::Pacer;

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

/// A frame's work on the render thread; returns how long of it was spent
/// waiting for the display (for a swap chain image), which isn't cost.
type Job<T> = Box<dyn FnOnce(&mut T) -> Duration + Send>;

/// A job done: when, and how long it waited for the display.
type Done<T> = (T, Instant, Duration);

/// How the thread ended: a panic's payload, raised again on the main
/// thread as `JoinHandle::join` would give it.
type Ended = std::thread::Result<()>;

pub(crate) struct RenderThread<T: Send + 'static> {
    /// The renderer's side, while no frame is in flight.
    home: Option<T>,
    jobs: Option<Sender<(T, Job<T>)>>,
    done: Receiver<Done<T>>,
    /// Sent once as the thread ends, and closed after.
    ended: Receiver<Ended>,
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
        let (ended_tx, ended) = channel::<Ended>();
        let body = move || {
            let run = std::panic::AssertUnwindSafe(|| {
                for (mut target, job) in inbox {
                    let display = job(&mut target);
                    if finished.send((target, Instant::now(), display)).is_err() {
                        return;
                    }
                }
            });
            let _ = ended_tx.send(std::panic::catch_unwind(run));
        };
        rook_platform::get()
            .threads
            .spawn("rook render", Box::new(body))
            .expect("couldn't start the render thread");
        Self {
            home: Some(target),
            jobs: Some(jobs),
            done,
            ended,
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
            Err(_) => match self.ended.recv() {
                Ok(Err(panic)) => std::panic::resume_unwind(panic),
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
    #[cfg(feature = "window")]
    pub(crate) fn start_next_at(&self) -> Option<Instant> {
        self.pacer.start_at(self.sent)
    }
}

impl<T: Send + 'static> Drop for RenderThread<T> {
    /// Take the renderer's side back, so it's dropped here (a window's
    /// surface with its window), then stop the thread.
    fn drop(&mut self) {
        if self.home.is_none()
            && let Ok((target, ..)) = self.done.recv()
        {
            self.home = Some(target);
        }
        self.jobs = None;
        let _ = self.ended.recv();
    }
}

#[cfg(test)]
mod tests;
