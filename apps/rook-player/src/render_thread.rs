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

type Job<T> = Box<dyn FnOnce(&mut T) + Send>;

pub(crate) struct RenderThread<T: Send + 'static> {
    /// The renderer's side, while no frame is in flight.
    home: Option<T>,
    jobs: Option<Sender<(T, Job<T>)>>,
    done: Receiver<(T, Duration)>,
    thread: Option<JoinHandle<()>>,
    /// When the frame in flight was handed over.
    sent: Option<Instant>,
    /// How long the thread took with the last frame it finished, waits
    /// for the display included.
    took: Duration,
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
                    let start = Instant::now();
                    job(&mut target);
                    if finished.send((target, start.elapsed())).is_err() {
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
            took: Duration::ZERO,
        }
    }

    /// The renderer's side, waiting for the frame in flight to finish
    /// first: the hand-over between frames.
    pub(crate) fn get(&mut self) -> &mut T {
        if self.home.is_none() {
            profiling::scope!("wait for the render thread");
            match self.done.recv() {
                Ok((target, took)) => {
                    self.home = Some(target);
                    self.took = took;
                    self.sent = None;
                }
                // The thread panicked: panic here with its message.
                Err(_) => match self.thread.take().map(JoinHandle::join) {
                    Some(Err(panic)) => std::panic::resume_unwind(panic),
                    _ => panic!("the render thread stopped"),
                },
            }
        }
        self.home.as_mut().expect("home once the frame finished")
    }

    /// Run `job` on the render thread with the renderer's side, which
    /// stays there until [`RenderThread::get`] takes it back.
    pub(crate) fn send(&mut self, job: impl FnOnce(&mut T) + Send + 'static) {
        self.get();
        let target = self.home.take().expect("got above");
        self.sent = Some(Instant::now());
        let jobs = self.jobs.as_ref().expect("open until dropped");
        if jobs.send((target, Box::new(job))).is_err() {
            // Only a panic on the thread closes it; `get` raises it.
            self.get();
        }
    }

    /// When the frame in flight should finish, going by how long the
    /// last one took; `None` with none in flight.
    pub(crate) fn due(&self) -> Option<Instant> {
        self.sent.map(|sent| sent + self.took)
    }
}

impl<T: Send + 'static> Drop for RenderThread<T> {
    /// Take the renderer's side back, so it's dropped here (a window's
    /// surface with its window), then stop the thread.
    fn drop(&mut self) {
        if self.home.is_none()
            && self.thread.is_some()
            && let Ok((target, _)) = self.done.recv()
        {
            self.home = Some(target);
        }
        self.jobs = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// When the main thread should start its next frame so that it is ready
/// just as the render thread finishes the frame in flight: as late as it
/// can, so the input and camera it takes are as fresh as a frame drawn
/// one after another would have them (the just-in-time start NVIDIA
/// Reflex and AMD Anti-Lag make, which Unreal's late update takes further
/// for VR poses). `None`: start now.
///
/// `main` is how long the main thread's part of a frame takes, up to the
/// hand-over. Guessing it short only makes the main thread wait at the
/// hand-over, the input that much older; guessing it long only leaves the
/// render thread that much idle.
pub(crate) fn start_at(render_due: Option<Instant>, main: Duration) -> Option<Instant> {
    render_due.and_then(|due| due.checked_sub(main))
}

/// How long the main thread's part of a frame takes, as [`start_at`]
/// guesses it: the longest of recent frames, falling slowly, so one quick
/// frame doesn't make the next start late.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct MainTime(Duration);

impl MainTime {
    pub(crate) fn note(&mut self, took: Duration) {
        self.0 = took.max(self.0.mul_f64(0.9));
    }

    pub(crate) fn get(self) -> Duration {
        self.0
    }
}

#[cfg(test)]
mod tests;
