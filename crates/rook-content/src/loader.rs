//! Resolving off the engine thread.
//!
//! The engine asks a [`Loader`] for references and collects the results at
//! a fixed point in its frame ([`Loader::poll`]), so disk, network and
//! decoding never hold up a frame and completions reach scripts on the
//! engine thread (architecture §8 "Content"). Worker threads start with the
//! first request: a place that never asks for content costs nothing.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use crate::image::Image;
use crate::mesh::Mesh;
use crate::{ContentRef, Failure, FailureKind, Fetched, Resolver};

/// Worker threads per loader. Asset delivery answers in a few hundred
/// milliseconds per request, so a few in flight keep a place with hundreds
/// of assets moving without hammering the service.
const WORKERS: usize = 4;

/// Identifies one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ticket(pub u64);

type Outcome = Result<Fetched, Failure>;

/// What a request wants from the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Want {
    /// The bytes as they are.
    Bytes,
    /// A mesh: the worker decodes it into [`Fetched::mesh`], and bytes that
    /// don't decode fail with [`FailureKind::Invalid`].
    Mesh,
    /// An image: the worker decodes it to RGBA8 with mips into
    /// [`Fetched::image`]; bytes that aren't an image fail with
    /// [`FailureKind::Invalid`].
    Image,
}

impl Want {
    /// What a content property must hold: a mesh for the `MeshId` of a
    /// mesh class or MeshPart (`mesh_owner`; files store it as
    /// `MeshContent`, some old MeshParts as `MeshID`) and for a union's
    /// `AssetId` (also `mesh_owner`; files save it as `AssetId`, Rook reads it as `Content`), an image for the image properties
    /// (textures, images, sky faces, the sun and moon), and anything for
    /// the rest (sounds, animations, scripts).
    pub fn for_property(name: &str, mesh_owner: bool) -> Self {
        if mesh_owner
            && matches!(
                name,
                "MeshContent" | "MeshId" | "MeshID" | "Content" | "AssetId"
            )
        {
            Self::Mesh
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
    jobs: Sender<Job>,
    done: Receiver<(Job, Outcome)>,
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

    /// Queue a reference, asking for `want`.
    pub fn request_as(&mut self, r: ContentRef, want: Want) -> Ticket {
        self.next += 1;
        let ticket = Ticket(self.next);
        let job = (r, want);
        if let Some(tickets) = self.waiting.get_mut(&job) {
            tickets.push(ticket);
            return ticket;
        }
        self.waiting.insert(job.clone(), vec![ticket]);
        let workers = self.workers.get_or_insert_with(|| spawn(&self.resolver));
        // The workers live as long as `jobs`, so the send can't fail.
        let _ = workers.jobs.send(job);
        ticket
    }

    /// Requests not delivered yet (`ContentProvider.RequestQueueSize`).
    pub fn queue_size(&self) -> usize {
        self.waiting.values().map(Vec::len).sum()
    }

    pub fn is_idle(&self) -> bool {
        self.waiting.is_empty()
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

/// Resolve a job and, for a mesh or an image, decode it.
fn run(resolver: &Resolver, (r, want): &Job) -> Outcome {
    let mut fetched = resolver.resolve(r)?;
    let invalid = |e: &dyn std::fmt::Display| Failure::new(FailureKind::Invalid, e.to_string());
    match want {
        Want::Bytes => {}
        Want::Mesh => {
            profiling::scope!("decode mesh");
            let mesh = Mesh::decode(&fetched.bytes).map_err(|e| invalid(&e))?;
            fetched.mesh = Some(Arc::new(mesh));
        }
        // Material textures come decoded.
        Want::Image if fetched.image.is_some() => {}
        Want::Image => {
            profiling::scope!("decode image");
            let image = Image::decode(&fetched.bytes).map_err(|e| invalid(&e))?;
            fetched.image = Some(Arc::new(image));
        }
    }
    Ok(fetched)
}

fn spawn(resolver: &Arc<Resolver>) -> Workers {
    let (jobs, job_rx) = channel::<Job>();
    let (done_tx, done) = channel();
    let job_rx = Arc::new(Mutex::new(job_rx));
    for i in 0..WORKERS {
        let (resolver, jobs, done) = (resolver.clone(), job_rx.clone(), done_tx.clone());
        let spawned = std::thread::Builder::new()
            .name(format!("rook-content-{i}"))
            .spawn(move || {
                loop {
                    // Ends when the loader (and its sender) is dropped.
                    let Ok(job) = jobs.lock().expect("job queue").recv() else {
                        return;
                    };
                    let outcome = run(&resolver, &job);
                    if done.send((job, outcome)).is_err() {
                        return;
                    }
                }
            });
        // With no thread at all nothing would ever finish; one is enough.
        if spawned.is_err() && i > 0 {
            break;
        }
        spawned.expect("starting a content worker thread");
    }
    Workers { jobs, done }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, FailureKind, Install, InstallKind};

    #[test]
    fn resolves_off_thread_and_shares_duplicates() {
        let dir = crate::tests::temp_dir("loader");
        std::fs::create_dir_all(dir.join("content/textures")).unwrap();
        std::fs::write(dir.join("content/textures/face.png"), b"face").unwrap();
        let mut config = Config::empty();
        config.install = Install::at(&dir, InstallKind::Custom);
        let mut loader = Loader::new(Arc::new(Resolver::new(config)));
        assert!(loader.poll().is_empty());

        let face = ContentRef::File("textures/face.png".into());
        let a = loader.request(face.clone());
        let b = loader.request(face);
        let c = loader.request(ContentRef::Asset {
            id: 5,
            version: None,
        });
        assert_eq!(loader.queue_size(), 3);
        let results = loader.wait_all();
        assert_eq!(loader.queue_size(), 0);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, a);
        assert_eq!(results[1].0, b);
        assert_eq!(&*results[1].1.as_ref().unwrap().bytes, b"face");
        assert_eq!(results[2].0, c);
        assert_eq!(
            results[2].1.as_ref().unwrap_err().kind,
            FailureKind::Offline
        );
    }

    /// Asked for a mesh, the worker decodes it; bytes that aren't one fail
    /// as invalid, and the same file asked for as bytes still resolves.
    #[test]
    fn decodes_meshes_on_the_workers() {
        let dir = crate::tests::temp_dir("loader-mesh");
        std::fs::create_dir_all(dir.join("content/meshes")).unwrap();
        let text =
            "version 1.00\n1\n[2,0,0][0,1,0][0,0,0][0,2,0][0,1,0][0,0,0][0,0,2][0,1,0][0,0,0]\n";
        std::fs::write(dir.join("content/meshes/tri.mesh"), text).unwrap();
        std::fs::write(dir.join("content/meshes/face.png"), b"\x89PNG").unwrap();
        let mut config = Config::empty();
        config.install = Install::at(&dir, InstallKind::Custom);
        let mut loader = Loader::new(Arc::new(Resolver::new(config)));
        let tri = ContentRef::File("meshes/tri.mesh".into());
        loader.request_as(tri.clone(), Want::Mesh);
        loader.request_as(ContentRef::File("meshes/face.png".into()), Want::Mesh);
        loader.request(tri);
        let results = loader.wait_all();
        let mesh = results[0].1.as_ref().unwrap().mesh.as_ref().unwrap();
        assert_eq!(mesh.indices, [0, 1, 2]);
        assert_eq!(
            results[1].1.as_ref().unwrap_err().kind,
            FailureKind::Invalid
        );
        assert!(results[2].1.as_ref().unwrap().mesh.is_none());
    }
}
