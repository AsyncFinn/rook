//! Mesh files for drawing, loaded off the engine thread.
//!
//! The scene asks for a file when a part's mesh appears or changes
//! ([`MeshFiles::state`]); the content loader's workers resolve and decode
//! it, and [`MeshFiles::poll`] hands back what finished at a fixed point in
//! the frame, so a frame never waits for disk, network or decoding. Each
//! file is kept once by the hash of its bytes, however parts refer to it.
//! A place without file meshes starts no loader and pays nothing.

use std::collections::HashMap;
use std::sync::Arc;

use rook_content::{ContentRef, Loader, Resolver, Ticket, Want};
use rook_render::{Mesh, MeshHash};

/// A finished file and, if it loaded, its mesh.
pub type Finished = (ContentRef, Option<(MeshHash, Arc<Mesh>)>);

/// Where a mesh file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshState {
    Loading,
    Loaded(MeshHash),
    /// Not found, refused, not a mesh, or a version Rook can't read.
    Failed,
}

#[derive(Debug)]
pub struct MeshFiles {
    resolver: Arc<Resolver>,
    /// Made on the first request.
    loader: Option<Loader>,
    states: HashMap<ContentRef, MeshState>,
    tickets: HashMap<Ticket, ContentRef>,
}

impl MeshFiles {
    pub fn new(resolver: Arc<Resolver>) -> Self {
        Self {
            resolver,
            loader: None,
            states: HashMap::new(),
            tickets: HashMap::new(),
        }
    }

    /// A file's state, asking for it the first time.
    pub fn state(&mut self, r: &ContentRef) -> MeshState {
        if let Some(&state) = self.states.get(r) {
            return state;
        }
        let resolver = &self.resolver;
        let loader = self
            .loader
            .get_or_insert_with(|| Loader::new(resolver.clone()));
        self.tickets
            .insert(loader.request_as(r.clone(), Want::Mesh), r.clone());
        self.states.insert(r.clone(), MeshState::Loading);
        MeshState::Loading
    }

    /// Files finished since the last call, with their meshes (`None` for
    /// failures). With `wait`, blocks until nothing is loading (screenshots).
    pub fn poll(&mut self, wait: bool) -> Vec<Finished> {
        let Some(loader) = &mut self.loader else {
            return Vec::new();
        };
        if self.tickets.is_empty() {
            return Vec::new();
        }
        let finished = if wait {
            loader.wait_all()
        } else {
            loader.poll()
        };
        let mut out = Vec::new();
        for (ticket, outcome) in finished {
            let Some(r) = self.tickets.remove(&ticket) else {
                continue;
            };
            let loaded = outcome.ok().and_then(|f| {
                let hash = MeshHash(*f.hash.as_bytes());
                Some((hash, f.mesh?))
            });
            let state = match &loaded {
                Some((hash, _)) => MeshState::Loaded(*hash),
                None => MeshState::Failed,
            };
            self.states.insert(r.clone(), state);
            out.push((r, loaded));
        }
        out
    }
}
