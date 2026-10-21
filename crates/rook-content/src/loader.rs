//! Resolving off the engine thread.
//!
//! The engine asks a [`Loader`] for references and collects the results at
//! a fixed point in its frame ([`Loader::poll`]), so disk, network and
//! decoding never hold up a frame and completions reach scripts on the
//! engine thread (architecture §8 "Content"). Each loader's fetch threads
//! hand decoding to one process-wide pool ([`set_decode_threads`]), both at
//! below-normal priority and started by the first request. Requests carry a
//! priority that can change while they wait (the player ranks files by
//! distance from the camera); both stages take the most urgent job next.

mod queue;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use queue::Queue;

use crate::decode::audio::Audio;
use crate::decode::image::Image;
use crate::decode::mesh::Mesh;
use crate::{ContentRef, Failure, FailureKind, Fetched, Resolver};

/// Fetch threads per loader. Asset delivery answers in a few hundred
/// milliseconds per request, so a few in flight keep a place with hundreds
/// of assets moving without hammering the service; they wait rather than
/// compute, so they don't come out of the thread budget.
const FETCHERS: usize = 4;

/// Jobs queued or running on every loader in the process.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// When the last job in flight finished.
static IDLE_SINCE: Mutex<Option<Instant>> = Mutex::new(None);

/// Jobs queued or running on every loader in the process: 0 once
/// everything asked for has been fetched and decoded.
pub(crate) fn jobs_in_flight() -> usize {
    IN_FLIGHT.load(Ordering::Relaxed)
}

/// When every loader in the process last ran out of work, if none has
/// any now (`--bench` times loading by it).
pub fn idle_since() -> Option<Instant> {
    let since = *IDLE_SINCE.lock().expect("idle time");
    since.filter(|_| jobs_in_flight() == 0)
}

/// Count a job as finished.
fn job_done() {
    if IN_FLIGHT.fetch_sub(1, Ordering::Relaxed) == 1 {
        *IDLE_SINCE.lock().expect("idle time") = Some(Instant::now());
    }
}

/// Identifies one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ticket(pub u64);

type Outcome = Result<Fetched, Failure>;

/// What a request wants from the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Want {
    /// The bytes as they are.
    Bytes,
    /// A mesh: the worker decodes it into [`Fetched::mesh`]; bytes that
    /// don't decode fail with [`FailureKind::Invalid`], or
    /// [`FailureKind::UnknownVersion`] for a version Rook doesn't read yet.
    Mesh,
    /// An image: the worker decodes it to RGBA8 with mips into
    /// [`Fetched::image`]; bytes that aren't an image fail with
    /// [`FailureKind::Invalid`].
    Image,
    /// A sound: the worker decodes it to 16-bit PCM into
    /// [`Fetched::audio`] and frees the file, leaving [`Fetched::bytes`]
    /// empty; bytes that aren't a WAV, Ogg or MP3 sound fail with
    /// [`FailureKind::Invalid`].
    Audio,
}

impl Want {
    /// What a content property must hold: a mesh for the `MeshId` of a
    /// mesh class or MeshPart (`mesh_owner`; files store it as
    /// `MeshContent`, some old MeshParts as `MeshID`) and for a union's
    /// `AssetId` (also `mesh_owner`; files save it as `AssetId`, Rook reads it as `Content`), an image for the image properties
    /// (textures, images, sky faces, the sun and moon), a sound for
    /// Sound's `SoundId` (files store it as `AudioContent`), and anything
    /// for the rest (animations, scripts).
    pub fn for_property(name: &str, mesh_owner: bool) -> Self {
        if mesh_owner
            && matches!(
                name,
                "MeshContent" | "MeshId" | "MeshID" | "Content" | "AssetId"
            )
        {
            Self::Mesh
        } else if matches!(name, "SoundId" | "AudioContent") {
            Self::Audio
        } else if ["Texture", "Image", "ColorMap", "Skybox", "Sun", "Moon"]
            .iter()
            .any(|k| name.contains(k))
        {
            Self::Image
        } else {
            Self::Bytes
        }
    }
}

type Job = (ContentRef, Want);

#[derive(Debug)]
struct Workers {
    jobs: Arc<Queue<Job, ()>>,
    done: Receiver<(Job, Outcome)>,
}

impl Drop for Workers {
    /// The fetch threads end with their loader.
    fn drop(&mut self) {
        self.jobs.close();
    }
}

/// Queues references for the worker threads and hands back results.
#[derive(Debug)]
pub struct Loader {
    resolver: Arc<Resolver>,
    workers: Option<Workers>,
    next: u64,
    /// Tickets waiting on each reference in flight (one resolution serves
    /// them all).
    waiting: HashMap<Job, Vec<Ticket>>,
}

impl Loader {
    pub fn new(resolver: Arc<Resolver>) -> Self {
        Self {
            resolver,
            workers: None,
            next: 0,
            waiting: HashMap::new(),
        }
    }

    pub fn resolver(&self) -> &Arc<Resolver> {
        &self.resolver
    }

    /// Queue a reference for its bytes; the result comes from
    /// [`poll`](Self::poll).
    pub fn request(&mut self, r: ContentRef) -> Ticket {
        self.request_as(r, Want::Bytes)
    }

    /// Queue a reference, asking for `want`, ahead of anything with a
    /// priority.
    pub fn request_as(&mut self, r: ContentRef, want: Want) -> Ticket {
        self.request_at(r, want, 0.0)
    }

    /// Queue a reference, asking for `want`, behind requests of a lower
    /// `priority` (a reference already in flight keeps its own).
    pub fn request_at(&mut self, r: ContentRef, want: Want, priority: f32) -> Ticket {
        self.next += 1;
        let ticket = Ticket(self.next);
        let job = (r, want);
        if let Some(tickets) = self.waiting.get_mut(&job) {
            tickets.push(ticket);
            return ticket;
        }
        self.waiting.insert(job.clone(), vec![ticket]);
        let workers = self
            .workers
            .get_or_insert_with(|| spawn_fetchers(&self.resolver));
        IN_FLIGHT.fetch_add(1, Ordering::Relaxed);
        workers.jobs.push(job, (), priority);
        ticket
    }

    /// Change the priority of a request not picked up yet; nothing once a
    /// worker has it.
    pub fn set_priority(&self, r: ContentRef, want: Want, priority: f32) {
        if let Some(workers) = &self.workers {
            workers.jobs.set_priority(&(r, want), priority);
        }
    }

    /// Requests not delivered yet (`ContentProvider.RequestQueueSize`).
    pub fn queue_size(&self) -> usize {
        self.waiting.values().map(Vec::len).sum()
    }

    /// Results finished since the last call, without waiting.
    pub fn poll(&mut self) -> Vec<(Ticket, Outcome)> {
        let mut out = Vec::new();
        let Some(workers) = &self.workers else {
            return out;
        };
        while let Ok((job, outcome)) = workers.done.try_recv() {
            for ticket in self.waiting.remove(&job).unwrap_or_default() {
                out.push((ticket, outcome.clone()));
            }
        }
        out.sort_by_key(|(t, _)| *t);
        out
    }

    /// Wait for every request in flight (tools and tests).
    pub fn wait_all(&mut self) -> Vec<(Ticket, Outcome)> {
        let mut out = Vec::new();
        while !self.waiting.is_empty() {
            let Some(workers) = &self.workers else { break };
            let Ok((job, outcome)) = workers.done.recv() else {
                break;
            };
            for ticket in self.waiting.remove(&job).unwrap_or_default() {
                out.push((ticket, outcome.clone()));
            }
        }
        out.sort_by_key(|(t, _)| *t);
        out
    }
}

/// Whether a job's bytes still need decoding once fetched.
fn needs_decoding(want: Want, fetched: &Fetched) -> bool {
    match want {
        Want::Bytes => false,
        // Material textures come decoded.
        Want::Image => fetched.image.is_none(),
        Want::Mesh | Want::Audio => true,
    }
}

/// Decode a fetched mesh, image or sound.
fn decode(want: Want, mut fetched: Fetched) -> Outcome {
    let invalid = |e: &dyn std::fmt::Display| Failure::new(FailureKind::Invalid, e.to_string());
    match want {
        Want::Bytes => {}
        Want::Mesh => {
            profiling::scope!("decode mesh");
            let mesh = Mesh::decode(&fetched.bytes).map_err(|e| {
                if e.is_unsupported() {
                    Failure::new(FailureKind::UnknownVersion, e.to_string())
                } else {
                    invalid(&e)
                }
            })?;
            fetched.mesh = Some(Arc::new(mesh));
        }
        Want::Image => {
            profiling::scope!("decode image");
            let image = Image::decode(&fetched.bytes).map_err(|e| invalid(&e))?;
            fetched.image = Some(Arc::new(image));
        }
        Want::Audio => {
            profiling::scope!("decode audio");
            let audio = Audio::decode(&fetched.bytes).map_err(|e| invalid(&e))?;
            fetched.audio = Some(Arc::new(audio));
            // Nothing reads a sound's file once it's decoded, and freeing
            // a few megabytes costs a fraction of a millisecond: free it
            // here rather than on the engine thread, where a place's sounds
            // finishing together would add up to a stalled frame.
            fetched.bytes = Arc::from([]);
        }
    }
    Ok(fetched)
}

type Done = Sender<(Job, Outcome)>;

/// Hand a finished job back to its loader.
fn finish(done: &Done, job: Job, outcome: Outcome) -> bool {
    job_done();
    done.send((job, outcome)).is_ok()
}

/// Start `count` threads named `name-i`, at below-normal priority, each
/// running `work` on the most urgent item of a shared queue until it
/// closes.
fn spawn_threads<K, V>(
    name: &str,
    count: usize,
    queue: &Arc<Queue<K, V>>,
    work: impl Fn(K, V, f32) -> bool + Send + Clone + 'static,
) where
    K: std::hash::Hash + Eq + Clone + Send + 'static,
    V: Send + 'static,
{
    for i in 0..count {
        let (queue, work) = (queue.clone(), work.clone());
        #[expect(
            clippy::disallowed_types,
            reason = "the content cache, install lookup and decode workers are rook-platform's known exception (architecture §2)"
        )]
        let spawned = std::thread::Builder::new()
            .name(format!("{name}-{i}"))
            .spawn(move || {
                lower_priority();
                while let Some((key, value, priority)) = queue.pop() {
                    if !work(key, value, priority) {
                        return;
                    }
                }
            });
        // With no thread at all nothing would ever finish; one is enough.
        if spawned.is_err() && i > 0 {
            break;
        }
        spawned.expect("starting a content thread");
    }
}

/// Decode threads; 1 until the engine sets it.
static DECODE_THREADS: AtomicUsize = AtomicUsize::new(1);

/// Size the decode pool. Takes effect if the pool hasn't started yet
/// (it starts with the first decode).
pub fn set_decode_threads(threads: usize) {
    DECODE_THREADS.store(threads.max(1), Ordering::Relaxed);
}

/// Run the calling thread at below-normal priority, so background work
/// yields to the frame and to audio when cores are short. Only Windows for
/// now; elsewhere the thread keeps its priority.
#[allow(unsafe_code)]
fn lower_priority() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        // SAFETY: GetCurrentThread returns a pseudo-handle for the calling
        // thread that is always valid and needs no closing. A failure only
        // leaves the priority as it was.
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
        }
    }
}

/// Decoding for a job, run on the decode pool.
type DecodeJob = Box<dyn FnOnce() + Send>;

/// Queue decoding on the process's decode pool, starting it the first
/// time, behind decoding of a lower `priority` (the fetch's, as it was when
/// fetching began).
fn queue_decode(job: DecodeJob, priority: f32) {
    static POOL: OnceLock<Arc<Queue<u64, DecodeJob>>> = OnceLock::new();
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let pool = POOL.get_or_init(|| {
        let queue = Arc::new(Queue::new());
        spawn_threads(
            "rook-decode",
            DECODE_THREADS.load(Ordering::Relaxed),
            &queue,
            |_, job: DecodeJob, _| {
                job();
                true
            },
        );
        queue
    });
    pool.push(NEXT.fetch_add(1, Ordering::Relaxed), job, priority);
}

fn spawn_fetchers(resolver: &Arc<Resolver>) -> Workers {
    let jobs = Arc::new(Queue::new());
    let (done_tx, done) = channel();
    let resolver = resolver.clone();
    spawn_threads(
        "rook-content",
        FETCHERS,
        &jobs,
        move |job: Job, (), priority| {
            let fetched = resolver.resolve(&job.0);
            match fetched {
                Ok(fetched) if needs_decoding(job.1, &fetched) => {
                    let done = done_tx.clone();
                    queue_decode(
                        Box::new(move || {
                            let outcome = decode(job.1, fetched);
                            finish(&done, job, outcome);
                        }),
                        priority,
                    );
                    true
                }
                outcome => finish(&done_tx, job, outcome),
            }
        },
    );
    Workers { jobs, done }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "tests set up files on the machine running them"
)]
mod tests;
