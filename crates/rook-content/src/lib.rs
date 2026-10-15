//! Content for Rook: what the strings in `MeshId`, `TextureId`, `SoundId`
//! and the other content properties refer to, and their bytes.
//!
//! - [`ContentRef`] parses a content string once (`rbxasset://`,
//!   `rbxassetid://`, legacy `http://www.roblox.com/asset/?id=` URLs,
//!   `rbxthumb://`).
//! - A [`Resolver`] turns a reference into bytes through a chain of
//!   providers, in a configurable order: Rook's own defaults, the user's
//!   Roblox install, folders the user points at, and Roblox's asset
//!   delivery (anonymous, opt-out, with a disk [`Cache`]). Each answer says
//!   where the bytes came from.
//! - A [`Loader`] runs resolutions on worker threads and hands the results
//!   back when the engine asks, so the engine thread never waits for disk
//!   or network.
//!
//! Decoders for what the bytes hold live here too, one per kind: [`mesh`]
//! reads mesh files, [`image`] image files and [`audio`] sound files. See `docs/design/architecture.md`
//! §8 "Content" and ROADMAP M4.

pub mod audio;
pub mod cache;
pub mod defaults;
pub mod delivery;
pub mod image;
pub mod install;
mod loader;
pub mod materials;
pub mod mesh;
mod reference;
mod resolver;

pub use cache::Cache;
pub use install::{Install, InstallKind};
pub use loader::{Loader, Ticket, Want};
pub use reference::{ContentRef, MaterialMap, Thumbnail, image_url_rewrite};
pub use resolver::{Config, ProviderKind, Resolver};

use std::fmt;
use std::sync::Arc;

/// Where a resolved reference's bytes came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceKind {
    /// Content Rook ships itself.
    Defaults,
    /// The user's Roblox install.
    Install,
    /// A folder the user pointed Rook at (`ROOK_CONTENT_DIRS`).
    Folder,
    /// Rook's disk cache of earlier downloads.
    Cache,
    /// Downloaded from Roblox's asset delivery just now.
    Delivery,
}

impl SourceKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Defaults => "rook defaults",
            Self::Install => "roblox install",
            Self::Folder => "content folder",
            Self::Cache => "cache",
            Self::Delivery => "asset delivery",
        }
    }
}

/// Where bytes came from: the provider and the file or URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub kind: SourceKind,
    /// A file path, or the URL downloaded (without its signature). For the
    /// cache, the blob's path and, after a space, the URL it came from.
    pub location: String,
}

/// A resolved reference.
#[derive(Debug, Clone)]
pub struct Fetched {
    /// The file; empty when a [`Loader`] decoded it as a sound
    /// ([`Want::Audio`]).
    pub bytes: Arc<[u8]>,
    /// BLAKE3 of the bytes (as resolved, even once freed).
    pub hash: blake3::Hash,
    pub source: Source,
    /// The decoded mesh, when a [`Loader`] was asked for a mesh
    /// ([`Want::Mesh`]); decoding happens on its worker threads.
    pub mesh: Option<Arc<mesh::Mesh>>,
    /// The decoded image, when a [`Loader`] was asked for an image
    /// ([`Want::Image`]).
    pub image: Option<Arc<image::Image>>,
    /// The decoded sound, when a [`Loader`] was asked for one
    /// ([`Want::Audio`]).
    pub audio: Option<Arc<audio::Audio>>,
}

impl Fetched {
    fn new(bytes: impl Into<Arc<[u8]>>, kind: SourceKind, location: String) -> Self {
        let bytes = bytes.into();
        Self {
            hash: blake3::hash(&bytes),
            bytes,
            source: Source { kind, location },
            mesh: None,
            image: None,
            audio: None,
        }
    }
}

/// Why a reference didn't resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FailureKind {
    /// Nowhere has it: no such file in the install, or asset delivery says
    /// the asset doesn't exist.
    NotFound,
    /// Asset delivery refuses it (not public, or needs an account).
    Forbidden,
    /// A form Rook doesn't resolve (thumbnails, asset version ids).
    Unsupported,
    /// It needs the network, and asset delivery is off.
    Offline,
    /// The network failed (no connection, timeout, an unexpected answer).
    Network,
    /// The bytes aren't what was asked for: not a mesh or an image, or one
    /// that is damaged or of a version Rook doesn't read yet.
    Invalid,
}

impl FailureKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::NotFound => "not-found",
            Self::Forbidden => "forbidden",
            Self::Unsupported => "unsupported",
            Self::Offline => "offline",
            Self::Network => "network",
            Self::Invalid => "invalid",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::NotFound,
            Self::Forbidden,
            Self::Unsupported,
            Self::Offline,
            Self::Network,
            Self::Invalid,
        ]
        .into_iter()
        .find(|k| k.name() == name)
    }

    /// Whether the answer will hold for a while, so the cache remembers it.
    pub fn is_lasting(self) -> bool {
        matches!(
            self,
            Self::NotFound | Self::Forbidden | Self::Unsupported | Self::Invalid
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
}

impl Failure {
    pub fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.name(), self.message)
    }
}

impl std::error::Error for Failure {}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    /// A fresh, empty folder for one test.
    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("rook-content-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
