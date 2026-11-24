//! The loader's threads: fetchers that resolve requests by priority and
//! decoders that turn what they fetched into what was wanted.

use super::*;

#[derive(Debug)]
pub(super) struct Workers {
    pub(super) jobs: Arc<Queue<Job, ()>>,
    pub(super) done: Receiver<(Job, Outcome)>,
}

impl Drop for Workers {
    fn drop(&mut self) {
        self.jobs.close();
    }
}

pub(super) fn needs_decoding(want: Want, fetched: &Fetched) -> bool {
    match want {
        Want::Bytes => false,
        // Material textures come decoded.
        Want::Image => fetched.image.is_none(),
        Want::Mesh | Want::Audio | Want::Model => true,
    }
}

pub(super) fn decode(want: Want, mut fetched: Fetched) -> Outcome {
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
            let image = Image::decode_texture(&fetched.bytes).map_err(|e| invalid(&e))?;
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
        Want::Model => {
            profiling::scope!("decode model");
            let model = rook_formats::load(&fetched.bytes).map_err(|e| invalid(&e))?;
            if let Some(compile) = SCRIPT_COMPILER.get() {
                profiling::scope!("compile model scripts");
                model.script_sources().for_each(compile);
            }
            fetched.model = Some(Arc::new(model));
        }
    }
    Ok(fetched)
}

pub(super) type Done = Sender<(Job, Outcome)>;

pub(super) fn finish(done: &Done, job: Job, outcome: Outcome) -> bool {
    job_done();
    done.send((job, outcome)).is_ok()
}

/// Start `count` threads named `name-i`, at below-normal priority, each
/// running `work` on the most urgent item of a shared queue until it
/// closes.
pub(super) fn spawn_threads<K, V>(
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
            reason = "the content cache, install lookup and decode workers are rook-platform's known exception (docs/design/architecture.md#crate-layout)"
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
pub(super) static DECODE_THREADS: AtomicUsize = AtomicUsize::new(1);

/// Size the decode pool. Takes effect if the pool hasn't started yet
/// (it starts with the first decode).
pub fn set_decode_threads(threads: usize) {
    DECODE_THREADS.store(threads.max(1), Ordering::Relaxed);
}

/// What compiles the scripts in the models workers read; nothing until
/// the engine sets it.
pub(super) static SCRIPT_COMPILER: OnceLock<fn(&str)> = OnceLock::new();

/// Have the decode workers hand each script source in the models they
/// read ([`Want::Model`]) to `compile`, so running those scripts later
/// compiles nothing on the engine thread. The first call sets it.
pub fn set_script_compiler(compile: fn(&str)) {
    _ = SCRIPT_COMPILER.set(compile);
}

/// Run the calling thread at below-normal priority, so background work
/// yields to the frame and to audio when cores are short. Elsewhere than
/// Windows the thread keeps its priority.
#[allow(unsafe_code)]
pub(super) fn lower_priority() {
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

pub(super) type DecodeJob = Box<dyn FnOnce() + Send>;

/// Queue decoding on the process's decode pool, starting it the first
/// time, behind decoding of a lower `priority` (the fetch's, as it was when
/// fetching began).
pub(super) fn queue_decode(job: DecodeJob, priority: f32) {
    pub(super) static POOL: OnceLock<Arc<Queue<u64, DecodeJob>>> = OnceLock::new();
    pub(super) static NEXT: AtomicU64 = AtomicU64::new(0);
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

pub(super) fn spawn_fetchers(resolver: &Arc<Resolver>) -> Workers {
    let jobs = Arc::new(Queue::new());
    let (done_tx, done) = channel();
    let resolver = resolver.clone();
    spawn_threads(
        "rook-content",
        FETCHERS,
        &jobs,
        move |job: Job, (), priority| {
            let fetched = resolver.resolve_as(&job.0, job.1);
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
