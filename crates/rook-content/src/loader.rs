//! Resolving off the engine thread.
//!
//! The engine asks a [`Loader`] for references and collects the results at
//! a fixed point in its frame ([`Loader::poll`]), so disk, network and
//! decoding never hold up a frame and completions reach scripts on the
//! engine thread (docs/design/architecture/content.md). Each loader's fetch threads
//! hand decoding to one process-wide pool ([`set_decode_threads`]), both at
//! below-normal priority and started by the first request. Requests carry a
//! priority that can change while they wait (the player ranks files by
//! distance from the camera); both stages take the most urgent job next.

mod queue;
mod workers;

use workers::*;
pub use workers::{set_decode_threads, set_script_compiler};

use rustc_hash::FxHashMap;
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

fn job_done() {
    if IN_FLIGHT.fetch_sub(1, Ordering::Relaxed) == 1 {
        *IDLE_SINCE.lock().expect("idle time") = Some(Instant::now());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ticket(pub u64);

type Outcome = Result<Fetched, Failure>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Want {
    /// The bytes as they are.
    Bytes,
    /// A mesh: the worker decodes it into [`Fetched::mesh`]; bytes that
    /// don't decode fail with [`FailureKind::Invalid`], or
    /// [`FailureKind::UnknownVersion`] for a version Rook doesn't read.
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
    /// A model file (what `require(assetId)` and `InsertService:LoadAsset`
    /// put in): the worker reads it into [`Fetched::model`]; bytes that
    /// aren't a place or model file fail with [`FailureKind::Invalid`].
    Model,
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
pub struct Loader {
    resolver: Arc<Resolver>,
    workers: Option<Workers>,
    next: u64,
    /// Tickets waiting on each reference in flight (one resolution serves
    /// them all).
    waiting: FxHashMap<Job, Vec<Ticket>>,
}

impl Loader {
    pub fn new(resolver: Arc<Resolver>) -> Self {
        Self {
            resolver,
            workers: None,
            next: 0,
            waiting: FxHashMap::default(),
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

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "tests set up files on the machine running them"
)]
mod tests;
