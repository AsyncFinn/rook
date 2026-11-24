//! Mesh files and images for drawing, loaded off the engine thread.
//!
//! The scene asks for a file when a part's mesh or image appears or
//! changes ([`Files::state`]); the content loader's workers resolve and
//! decode it, and [`Files::poll`] hands back what finished at a fixed point
//! in the frame, so a frame never waits for disk, network or decoding. Each
//! file is kept once by the hash of its bytes; one that fails keeps why
//! ([`Files::failure`]) for the load report. A texture loads small
//! (`Want::Texture(0)`) and [`Files::upgrade`] asks for it larger as it
//! grows on screen; the larger version replaces it under the hash it first
//! loaded by ([`Files::take_upgraded`]).

use rustc_hash::FxHashMap;
use std::sync::Arc;

use rook_content::decode::image::Image;
use rook_content::decode::mesh::Mesh;
use rook_content::{ContentRef, Failure, FailureKind, Loader, Resolver, Ticket, Want};
use rook_render::{ImageHash, MeshHash};

pub(crate) type FileRef = (ContentRef, Want);

#[derive(Debug, Clone)]
pub(crate) enum Loaded {
    Mesh(MeshHash, Arc<Mesh>),
    Image(ImageHash, Arc<Image>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileState {
    Loading,
    /// Loaded, by the hash of its bytes.
    Loaded([u8; 32]),
    /// Not found, refused, or not what was asked for (not a mesh or an
    /// image, or a version Rook can't read).
    Failed,
}

/// A texture that loaded small, by the hash it first loaded under.
#[derive(Debug)]
struct Streamed {
    r: ContentRef,
    /// The size asked for so far, as `Want::Texture`'s exponent.
    asked: u8,
    /// Its largest level's longer side now.
    side: u32,
    /// It has no larger version: the last one asked for came no larger,
    /// or failed.
    whole: bool,
    loading: bool,
}

#[derive(Debug)]
pub(crate) struct Files {
    resolver: Arc<Resolver>,
    /// Made on the first request.
    loader: Option<Loader>,
    states: FxHashMap<FileRef, FileState>,
    failures: FxHashMap<FileRef, Failure>,
    tickets: FxHashMap<Ticket, FileRef>,
    streamed: FxHashMap<ImageHash, Streamed>,
    /// Larger versions of textures being loaded.
    upgrades: FxHashMap<Ticket, ImageHash>,
    /// Larger versions that arrived, for [`Files::take_upgraded`].
    upgraded: Vec<(ImageHash, Arc<Image>)>,
}

impl Files {
    pub(crate) fn new(resolver: Arc<Resolver>) -> Self {
        Self {
            resolver,
            loader: None,
            states: FxHashMap::default(),
            failures: FxHashMap::default(),
            tickets: FxHashMap::default(),
            streamed: FxHashMap::default(),
            upgrades: FxHashMap::default(),
            upgraded: Vec::new(),
        }
    }

    /// Ask for a texture that loaded small (`Want::Texture(0)`) at least
    /// `1 << size` pixels across, behind files of a lower `priority`,
    /// unless it's that large already, has no larger version, or is
    /// already being loaded larger. Returns whether it asked.
    pub(crate) fn upgrade(&mut self, hash: ImageHash, size: u8, priority: f32) -> bool {
        let Some(s) = self.streamed.get_mut(&hash) else {
            return false;
        };
        let side = 1u32.checked_shl(u32::from(size)).unwrap_or(u32::MAX);
        if s.whole || s.loading || size <= s.asked || s.side >= side {
            return false;
        }
        let Some(loader) = &mut self.loader else {
            return false;
        };
        let ticket = loader.request_at(s.r.clone(), Want::Texture(size), priority);
        self.upgrades.insert(ticket, hash);
        s.asked = size;
        s.loading = true;
        true
    }

    /// The larger versions of textures that arrived since the last call,
    /// each under the hash it first loaded by.
    pub(crate) fn take_upgraded(&mut self) -> Vec<(ImageHash, Arc<Image>)> {
        std::mem::take(&mut self.upgraded)
    }

    fn upgrade_done(&mut self, hash: ImageHash, outcome: Result<rook_content::Fetched, Failure>) {
        let Some(s) = self.streamed.get_mut(&hash) else {
            return;
        };
        s.loading = false;
        let Some(image) = outcome.ok().and_then(|f| f.image) else {
            s.whole = true;
            return;
        };
        let side = image.width.max(image.height);
        let asked = 1u32.checked_shl(u32::from(s.asked)).unwrap_or(u32::MAX);
        if side < asked {
            s.whole = true;
        }
        if side > s.side {
            s.side = side;
            self.upgraded.push((hash, image));
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

    pub(crate) fn failure(&self, key: &FileRef) -> Option<&Failure> {
        self.failures.get(key)
    }

    pub(crate) fn idle(&self) -> bool {
        self.tickets.is_empty() && self.upgrades.is_empty()
    }

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
        if self.tickets.is_empty() && self.upgrades.is_empty() {
            return Vec::new();
        }
        let finished = if wait {
            loader.wait_all()
        } else {
            loader.poll()
        };
        let mut out = Vec::new();
        for (ticket, outcome) in finished {
            if let Some(hash) = self.upgrades.remove(&ticket) {
                self.upgrade_done(hash, outcome);
                continue;
            }
            let Some(key) = self.tickets.remove(&ticket) else {
                continue;
            };
            let loaded = outcome.and_then(|f| {
                let hash = *f.hash.as_bytes();
                let loaded = match key.1 {
                    Want::Mesh => f.mesh.map(|m| Loaded::Mesh(MeshHash(hash), m)),
                    Want::Image | Want::Texture(_) => {
                        f.image.map(|i| Loaded::Image(ImageHash(hash), i))
                    }
                    Want::Bytes | Want::Audio | Want::Model => None,
                };
                loaded.ok_or_else(|| Failure::new(FailureKind::Invalid, "not a mesh or an image"))
            });
            if let (Want::Texture(0), Ok(Loaded::Image(hash, image))) = (key.1, &loaded) {
                self.streamed.entry(*hash).or_insert_with(|| Streamed {
                    r: key.0.clone(),
                    asked: 0,
                    side: image.width.max(image.height),
                    whole: false,
                    loading: false,
                });
            }
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
