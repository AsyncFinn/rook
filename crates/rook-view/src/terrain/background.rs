//! Meshing chunks off the frame: one thread of its own, meshing a job's
//! chunks one after another, so a frame never waits for a ring the camera
//! crossed and the cores the frame's own work spreads over stay free
//! (Studio, too, catches its rings up over a few frames:
//! `docs/behavior/terrain/detail.md`). A host without threads meshes on
//! the calling thread instead.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use glam::IVec2;
use rook_terrain::{Detail, mesh_chunk_at};

use super::*;

/// What a job is for: the rings and voxels it was asked for, and what
/// should be drawn once it's done.
#[derive(Debug)]
pub(super) struct Job {
    pub(super) epoch: u64,
    pub(super) rings: Rings,
    /// The chunks to draw, by level.
    pub(super) draw: HashSet<LevelChunk>,
    /// The level-0 chunks whose Grass tops to keep, and the camera's
    /// level-0 column they're around (`None` without blades).
    pub(super) tops: HashSet<IVec3>,
    pub(super) near: Option<IVec2>,
}

#[derive(Debug)]
struct Request {
    levels: Arc<Vec<Grid>>,
    chunks: Vec<LevelChunk>,
}

#[derive(Debug)]
struct Worker {
    requests: Sender<Request>,
    done: Receiver<Vec<ChunkMesh>>,
}

/// The mesher's thread, started with the first job, and the job it's on.
#[derive(Debug, Default)]
pub(super) struct Background {
    worker: Option<Worker>,
    /// Whether starting the thread failed (meshing happens in place).
    alone: bool,
    job: Option<Job>,
}

/// Mesh `chunks` of `levels`, each at its level, with skirts.
pub(super) fn mesh(levels: &[Grid], chunks: &[LevelChunk]) -> Vec<ChunkMesh> {
    chunks
        .iter()
        .map(|&(l, c)| mesh_chunk_at(&levels[usize::from(l)], c, Detail::at(l)))
        .collect()
}

impl Background {
    pub(super) fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Mesh `chunks` of `levels` for `job` off the frame. Returns the job
    /// back when there's no thread to mesh on.
    pub(super) fn start(
        &mut self,
        levels: &Arc<Vec<Grid>>,
        chunks: Vec<LevelChunk>,
        job: Job,
    ) -> Result<(), Box<Job>> {
        debug_assert!(self.job.is_none());
        let Some(worker) = self.worker() else {
            return Err(Box::new(job));
        };
        let request = Request {
            levels: levels.clone(),
            chunks,
        };
        if worker.requests.send(request).is_err() {
            self.worker = None;
            self.alone = true;
            return Err(Box::new(job));
        }
        self.job = Some(job);
        Ok(())
    }

    /// The job, with its meshes, if it's done.
    pub(super) fn poll(&mut self) -> Option<(Job, Vec<ChunkMesh>)> {
        let worker = self.worker.as_ref()?;
        self.job.as_ref()?;
        match worker.done.try_recv() {
            Ok(meshes) => Some((self.job.take().expect("a job"), meshes)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => self.lost(),
        }
    }

    /// Wait for the job, if there is one: once this returns, the thread
    /// holds no voxels.
    pub(super) fn wait(&mut self) -> Option<(Job, Vec<ChunkMesh>)> {
        let worker = self.worker.as_ref()?;
        self.job.as_ref()?;
        match worker.done.recv() {
            Ok(meshes) => Some((self.job.take().expect("a job"), meshes)),
            Err(_) => self.lost(),
        }
    }

    /// The thread ended (it panicked): its job is lost and meshing
    /// happens in place from now on.
    fn lost(&mut self) -> Option<(Job, Vec<ChunkMesh>)> {
        self.worker = None;
        self.alone = true;
        self.job = None;
        None
    }

    fn worker(&mut self) -> Option<&Worker> {
        if self.worker.is_none() && !self.alone {
            let (requests, inbox) = channel::<Request>();
            let (outbox, done) = channel();
            let body = Box::new(move || {
                while let Ok(request) = inbox.recv() {
                    let meshes = mesh(&request.levels, &request.chunks);
                    // Let go of the voxels before answering, so an edit
                    // waiting for this job can change them in place.
                    drop(request);
                    if outbox.send(meshes).is_err() {
                        break;
                    }
                }
            });
            match rook_platform::get().threads.spawn("terrain-lod", body) {
                Ok(()) => self.worker = Some(Worker { requests, done }),
                Err(e) => {
                    eprintln!("warning: terrain detail meshes in place: {e}");
                    self.alone = true;
                }
            }
        }
        self.worker.as_ref()
    }
}
