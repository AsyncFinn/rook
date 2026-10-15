//! Mesh files and images for drawing, loaded off the engine thread.
//!
//! The scene asks for a file when a part's mesh, mesh texture, decal or
//! texture appears or changes ([`Files::state`]); the content loader's
//! workers resolve and decode it (`Want::Mesh` or `Want::Image`), and
//! [`Files::poll`] hands back what finished at a fixed point in the frame,
//! so a frame never waits for disk, network or decoding. Each file is kept
//! once by the hash of its bytes, however parts refer to it. A place
//! without file meshes or images starts no loader and pays nothing.

use std::collections::HashMap;
use std::sync::Arc;

use rook_content::{ContentRef, Loader, Resolver, Ticket, Want};
use rook_render::{Image, ImageHash, Mesh, MeshHash};

/// A file the scene asked for, and what it wants of it.
pub type FileRef = (ContentRef, Want);

/// What a finished file holds.
#[derive(Debug, Clone)]
pub enum Loaded {
    Mesh(MeshHash, Arc<Mesh>),
    Image(ImageHash, Arc<Image>),
}

/// Where a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    Loading,
    /// Loaded, by the hash of its bytes.
    Loaded([u8; 32]),
    /// Not found, refused, or not what was asked for (not a mesh or an
    /// image, or a version Rook can't read).
    Failed,
}

#[derive(Debug)]
pub struct Files {
    resolver: Arc<Resolver>,
    /// Made on the first request.
    loader: Option<Loader>,
    states: HashMap<FileRef, FileState>,
    tickets: HashMap<Ticket, FileRef>,
}

impl Files {
    pub fn new(resolver: Arc<Resolver>) -> Self {
        Self {
            resolver,
            loader: None,
            states: HashMap::new(),
            tickets: HashMap::new(),
        }
    }

    /// A file's state, asking for it the first time.
    pub fn state(&mut self, r: &ContentRef, want: Want) -> FileState {
        let key = (r.clone(), want);
        if let Some(&state) = self.states.get(&key) {
            return state;
        }
        let resolver = &self.resolver;
        let loader = self
            .loader
            .get_or_insert_with(|| Loader::new(resolver.clone()));
        self.tickets
            .insert(loader.request_as(r.clone(), want), key.clone());
        self.states.insert(key, FileState::Loading);
        FileState::Loading
    }

    /// Files finished since the last call, with what they hold (`None` for
    /// failures). With `wait`, blocks until nothing is loading
    /// (screenshots).
    pub fn poll(&mut self, wait: bool) -> Vec<(FileRef, Option<Loaded>)> {
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
            let Some(key) = self.tickets.remove(&ticket) else {
                continue;
            };
            let loaded = outcome.ok().and_then(|f| {
                let hash = *f.hash.as_bytes();
                match key.1 {
                    Want::Mesh => Some(Loaded::Mesh(MeshHash(hash), f.mesh?)),
                    Want::Image => Some(Loaded::Image(ImageHash(hash), f.image?)),
                    Want::Bytes | Want::Audio | Want::Collision => None,
                }
            });
            let state = match &loaded {
                Some(Loaded::Mesh(MeshHash(h), _) | Loaded::Image(ImageHash(h), _)) => {
                    FileState::Loaded(*h)
                }
                None => FileState::Failed,
            };
            self.states.insert(key.clone(), state);
            out.push((key, loaded));
        }
        out
    }
}
