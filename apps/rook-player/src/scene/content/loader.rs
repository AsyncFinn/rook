//! Mesh files and images for drawing, loaded off the engine thread.
//!
//! The scene asks for a file when a part's mesh, mesh texture, decal or
//! texture appears or changes ([`Files::state`]); the content loader's
//! workers resolve and decode it (`Want::Mesh` or `Want::Image`), and
//! [`Files::poll`] hands back what finished at a fixed point in the frame,
//! so a frame never waits for disk, network or decoding. Each file is kept
//! once by the hash of its bytes, however parts refer to it. A place
//! without file meshes or images starts no loader and pays nothing. A file
//! that fails keeps why ([`Files::failure`]), for the scene's load report.

use std::collections::HashMap;
use std::sync::Arc;

use rook_content::decode::image::Image;
use rook_content::decode::mesh::Mesh;
use rook_content::{ContentRef, Failure, FailureKind, Loader, Resolver, Ticket, Want};
use rook_render::{ImageHash, MeshHash};

/// A file the scene asked for, and what it wants of it.
pub(crate) type FileRef = (ContentRef, Want);

/// What a finished file holds.
#[derive(Debug, Clone)]
pub(crate) enum Loaded {
    Mesh(MeshHash, Arc<Mesh>),
    Image(ImageHash, Arc<Image>),
}

/// Where a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileState {
    Loading,
    /// Loaded, by the hash of its bytes.
    Loaded([u8; 32]),
    /// Not found, refused, or not what was asked for (not a mesh or an
    /// image, or a version Rook can't read).
    Failed,
}

#[derive(Debug)]
pub(crate) struct Files {
    resolver: Arc<Resolver>,
    /// Made on the first request.
    loader: Option<Loader>,
    states: HashMap<FileRef, FileState>,
    /// Why each failed file failed.
    failures: HashMap<FileRef, Failure>,
    tickets: HashMap<Ticket, FileRef>,
}

impl Files {
    pub(crate) fn new(resolver: Arc<Resolver>) -> Self {
        Self {
            resolver,
            loader: None,
            states: HashMap::new(),
            failures: HashMap::new(),
            tickets: HashMap::new(),
        }
    }

    /// A file's state, asking for it the first time, behind files of a
    /// lower `priority`.
    pub(crate) fn state(&mut self, r: &ContentRef, want: Want, priority: f32) -> FileState {
        let key = (r.clone(), want);
        if let Some(&state) = self.states.get(&key) {
            return state;
        }
        let resolver = &self.resolver;
        let loader = self
            .loader
            .get_or_insert_with(|| Loader::new(resolver.clone()));
        self.tickets
            .insert(loader.request_at(r.clone(), want, priority), key.clone());
        self.states.insert(key, FileState::Loading);
        FileState::Loading
    }

    /// Why a file failed, if it did.
    pub(crate) fn failure(&self, key: &FileRef) -> Option<&Failure> {
        self.failures.get(key)
    }

    /// Whether nothing asked for is still loading.
    pub(crate) fn idle(&self) -> bool {
        self.tickets.is_empty()
    }

    /// Change the priority of a file still waiting to load.
    pub(crate) fn set_priority(&self, key: &FileRef, priority: f32) {
        if let Some(loader) = &self.loader {
            loader.set_priority(key.0.clone(), key.1, priority);
        }
    }

    /// Files finished since the last call, with what they hold or why they
    /// failed. With `wait`, blocks until nothing is loading (screenshots).
    pub(crate) fn poll(&mut self, wait: bool) -> Vec<(FileRef, Result<Loaded, Failure>)> {
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
            let loaded = outcome.and_then(|f| {
                let hash = *f.hash.as_bytes();
                let loaded = match key.1 {
                    Want::Mesh => f.mesh.map(|m| Loaded::Mesh(MeshHash(hash), m)),
                    Want::Image => f.image.map(|i| Loaded::Image(ImageHash(hash), i)),
                    Want::Bytes | Want::Audio => None,
                };
                loaded.ok_or_else(|| Failure::new(FailureKind::Invalid, "not a mesh or an image"))
            });
            let state = match &loaded {
                Ok(Loaded::Mesh(MeshHash(h), _) | Loaded::Image(ImageHash(h), _)) => {
                    FileState::Loaded(*h)
                }
                Err(failure) => {
                    self.failures.insert(key.clone(), failure.clone());
                    FileState::Failed
                }
            };
            self.states.insert(key.clone(), state);
            out.push((key, loaded));
        }
        out
    }
}
