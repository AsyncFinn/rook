//! A long sound's samples while something plays it: decoded once, on a
//! thread of their own and a chunk at a time, for every voice and reader
//! of the sound, and let go when the last of them is. The audio callback
//! only copies decoded samples, never decodes, as Unreal's mixer leaves
//! decoding to tasks ahead of it: decoding there cost every voice of a
//! sound a decoder of its own, and opening the file for a loop took
//! milliseconds the callback didn't have.

use std::panic::AssertUnwindSafe;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use super::stream::{Layout, Stream};
use super::{AudioFormat, CHUNK};

/// The sound's samples a chunk of [`CHUNK`] frames at a time, interleaved,
/// each there once decoded. The first chunk is the sound's head, kept
/// from the load, so its slot stays empty.
#[derive(Debug)]
pub(super) struct Decoded {
    chunks: Box<[OnceLock<Box<[i16]>>]>,
}

impl Decoded {
    pub(super) fn chunk(&self, i: usize) -> Option<&[i16]> {
        self.chunks.get(i)?.get().map(|c| &**c)
    }

    #[cfg(test)]
    pub(super) fn is_done(&self) -> bool {
        self.chunks.iter().skip(1).all(|c| c.get().is_some())
    }
}

/// What a decode needs of its sound, without keeping the sound itself.
pub(super) struct Source {
    pub format: AudioFormat,
    pub file: Arc<[u8]>,
    pub layout: Layout,
    pub frames: usize,
}

/// Start decoding `source` for whoever holds what this returns.
pub(super) fn start(source: Source) -> Arc<Decoded> {
    let decoded = Arc::new(Decoded {
        chunks: (0..source.frames.div_ceil(CHUNK))
            .map(|_| OnceLock::new())
            .collect(),
    });
    let job = Job::new(source, Arc::downgrade(&decoded));
    if let Some(mut job) = send(job) {
        // No thread to decode on: all of it now, on this one.
        while job.step() {}
    }
    decoded
}

/// Hand `job` to the decode thread, starting it the first time; the job
/// back if there's no thread to run it.
fn send(job: Job) -> Option<Job> {
    static JOBS: OnceLock<Option<Mutex<Sender<Job>>>> = OnceLock::new();
    let jobs = JOBS.get_or_init(|| {
        let (tx, rx) = channel();
        let body = Box::new(move || run(&rx));
        let started = rook_platform::get().threads.spawn("sound-decode", body);
        started.ok().map(|()| Mutex::new(tx))
    });
    match jobs {
        Some(tx) => {
            let tx = tx.lock().unwrap_or_else(PoisonError::into_inner);
            tx.send(job).err().map(|e| e.0)
        }
        None => Some(job),
    }
}

/// Decode a chunk of each sound in turn, so a sound that starts while
/// others decode starts decoding at once; each stays far ahead of its
/// voices, decoding running at a hundred times real time or more.
fn run(jobs: &Receiver<Job>) {
    let mut active: Vec<Job> = Vec::new();
    loop {
        if active.is_empty() {
            match jobs.recv() {
                Ok(job) => active.push(job),
                Err(_) => return,
            }
        }
        active.extend(jobs.try_iter());
        active.retain_mut(Job::step);
    }
}

struct Job {
    source: Source,
    into: Weak<Decoded>,
    stream: Option<Stream>,
    /// Decoded samples not yet in a chunk.
    pending: Vec<i16>,
    packet: Vec<i16>,
    /// The chunk the pending samples start.
    next: usize,
}

impl Job {
    fn new(source: Source, into: Weak<Decoded>) -> Self {
        Self {
            source,
            into,
            stream: None,
            pending: Vec::new(),
            packet: Vec::new(),
            next: 0,
        }
    }

    /// Decode the next chunk; false once there's no more to do: the sound
    /// is all decoded, nothing plays it any more, or it no longer decodes
    /// as it did when it loaded (the rest stays silent).
    fn step(&mut self) -> bool {
        let Some(into) = self.into.upgrade() else {
            return false;
        };
        std::panic::catch_unwind(AssertUnwindSafe(|| self.fill(&into))).unwrap_or(false)
    }

    fn fill(&mut self, into: &Decoded) -> bool {
        let channels = usize::from(self.source.layout.channels);
        let size = CHUNK * channels;
        if self.stream.is_none() {
            let s = &self.source;
            match Stream::open(s.format, s.file.clone(), Some(s.layout)) {
                Ok(stream) => self.stream = Some(stream),
                Err(_) => return false,
            }
        }
        let stream = self.stream.as_mut().expect("opened");
        while self.pending.len() < size {
            match stream.next(&mut self.packet) {
                Ok(true) => self.pending.extend_from_slice(&self.packet),
                Ok(false) | Err(_) => break,
            }
        }
        let end = self.pending.len() < size;
        // The load counted the frames to keep: the file's last block can
        // run past them.
        let left = (self.source.frames.saturating_sub(self.next * CHUNK)) * channels;
        let take = self.pending.len().min(size).min(left);
        let chunk: Box<[i16]> = self.pending.drain(..take).collect();
        if self.next > 0
            && let Some(slot) = into.chunks.get(self.next)
        {
            let _ = slot.set(chunk);
        }
        self.next += 1;
        !end && self.next < into.chunks.len()
    }
}
