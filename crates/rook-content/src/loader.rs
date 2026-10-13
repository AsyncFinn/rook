//! Resolving off the engine thread.
//!
//! The engine asks a [`Loader`] for references and collects the results at
//! a fixed point in its frame ([`Loader::poll`]), so disk and network never
//! hold up a frame and completions reach scripts on the engine thread
//! (architecture §8 "Content"). Worker threads start with the first
//! request: a place that never asks for content costs nothing.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use crate::{ContentRef, Failure, Fetched, Resolver};

/// Worker threads per loader. Asset delivery answers in a few hundred
/// milliseconds per request, so a few in flight keep a place with hundreds
/// of assets moving without hammering the service.
const WORKERS: usize = 4;

/// Identifies one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ticket(pub u64);

type Outcome = Result<Fetched, Failure>;

#[derive(Debug)]
struct Workers {
    jobs: Sender<ContentRef>,
    done: Receiver<(ContentRef, Outcome)>,
}

/// Queues references for the worker threads and hands back results.
#[derive(Debug)]
pub struct Loader {
    resolver: Arc<Resolver>,
    workers: Option<Workers>,
    next: u64,
    /// Tickets waiting on each reference in flight (one resolution serves
    /// them all).
    waiting: HashMap<ContentRef, Vec<Ticket>>,
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

    /// Queue a reference; its result comes from [`poll`](Self::poll).
    pub fn request(&mut self, r: ContentRef) -> Ticket {
        self.next += 1;
        let ticket = Ticket(self.next);
        if let Some(tickets) = self.waiting.get_mut(&r) {
            tickets.push(ticket);
            return ticket;
        }
        self.waiting.insert(r.clone(), vec![ticket]);
        let workers = self.workers.get_or_insert_with(|| spawn(&self.resolver));
        // The workers live as long as `jobs`, so the send can't fail.
        let _ = workers.jobs.send(r);
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
        while let Ok((r, outcome)) = workers.done.try_recv() {
            for ticket in self.waiting.remove(&r).unwrap_or_default() {
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
            let Ok((r, outcome)) = workers.done.recv() else {
                break;
            };
            for ticket in self.waiting.remove(&r).unwrap_or_default() {
                out.push((ticket, outcome.clone()));
            }
        }
        out.sort_by_key(|(t, _)| *t);
        out
    }
}

fn spawn(resolver: &Arc<Resolver>) -> Workers {
    let (jobs, job_rx) = channel::<ContentRef>();
    let (done_tx, done) = channel();
    let job_rx = Arc::new(Mutex::new(job_rx));
    for i in 0..WORKERS {
        let (resolver, jobs, done) = (resolver.clone(), job_rx.clone(), done_tx.clone());
        let spawned = std::thread::Builder::new()
            .name(format!("rook-content-{i}"))
            .spawn(move || {
                loop {
                    // Ends when the loader (and its sender) is dropped.
                    let Ok(r) = jobs.lock().expect("job queue").recv() else {
                        return;
                    };
                    let outcome = resolver.resolve(&r);
                    if done.send((r, outcome)).is_err() {
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
}
