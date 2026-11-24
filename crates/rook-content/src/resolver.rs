//! The provider chain: a reference goes to each provider in turn until one
//! has it.

pub mod builtin;
#[expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "the content cache, install lookup and decode workers are rook-platform's known exception (docs/design/architecture.md#crate-layout)"
)]
pub mod cache;
mod config;
pub mod defaults;
pub mod delivery;
mod download;
#[expect(
    clippy::disallowed_methods,
    reason = "the content cache, install lookup and decode workers are rook-platform's known exception (docs/design/architecture.md#crate-layout)"
)]
pub mod install;
pub mod kept;
#[expect(
    clippy::disallowed_methods,
    reason = "the content cache, install lookup and decode workers are rook-platform's known exception (docs/design/architecture.md#crate-layout)"
)]
mod overrides;
#[expect(
    clippy::disallowed_methods,
    reason = "the content cache, install lookup and decode workers are rook-platform's known exception (docs/design/architecture.md#crate-layout)"
)]
pub mod packages;
pub mod thumbnails;
mod web;

use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::materials::{self, TexturePack};
use crate::{ContentRef, Failure, FailureKind, Fetched, MaterialMap, SourceKind};
use cache::{Cache, Cached};
use delivery::Delivery;
use install::Install;
use packages::Packages;
use thumbnails::Thumbnails;

pub use config::{ApiKey, Config, ProviderKind, parse_switch};

/// Resolves references to bytes. Shared by every DataModel and the
/// [`Loader`](crate::Loader)'s workers.
#[derive(Debug)]
pub struct Resolver {
    order: Vec<ProviderKind>,
    install: Option<Install>,
    /// Roblox's packages, asked before the install.
    packages: Option<Arc<Packages>>,
    download_packages: bool,
    folders: Vec<PathBuf>,
    /// The user's file for each asset id they replace ([`overrides`]).
    overrides: FxHashMap<u64, PathBuf>,
    delivery: Option<Delivery>,
    thumbnails: Option<Thumbnails>,
    cache: Option<Cache>,
    kept: Option<Cache>,
    /// Failures this run that the disk cache doesn't keep (network errors,
    /// asset delivery being off), so they aren't retried every request.
    failed: Mutex<FxHashMap<ContentRef, Failure>>,
}

impl Resolver {
    pub fn new(config: Config) -> Self {
        let overrides = config
            .overrides_dir
            .filter(|_| config.order.contains(&ProviderKind::Overrides))
            .map(|dir| overrides::read(&dir))
            .unwrap_or_default();
        Self {
            order: config.order,
            install: config.install,
            packages: config.packages,
            download_packages: config.network,
            folders: config.folders,
            overrides,
            delivery: config
                .network
                .then(|| Delivery::new(config.api_key, &config.endpoints))
                .flatten(),
            thumbnails: config
                .network
                .then(|| Thumbnails::new(&config.endpoints))
                .flatten(),
            cache: config.cache_dir.and_then(|dir| {
                let cache = open_store(&dir)?;
                if let Some(max) = config.cache_max {
                    // Off the caller's thread: walking a big cache takes a
                    // while, and nothing waits on it. Blobs a resolution is
                    // reading can't be deleted on Windows, and a deleted one
                    // is fetched again.
                    let trimmed = cache.clone();
                    #[expect(clippy::disallowed_types, reason = "the content cache, install lookup and decode workers are rook-platform's known exception (docs/design/architecture.md#crate-layout)")]
                    let trim = std::thread::Builder::new()
                        .name("rook-cache-trim".into())
                        .spawn(move || trimmed.trim(max));
                    if let Err(e) = trim {
                        tracing::warn!(target: "content", "couldn't start trimming the cache: {e}");
                    }
                }
                Some(cache)
            }),
            kept: config.kept_dir.as_deref().and_then(open_store),
            failed: Mutex::new(FxHashMap::default()),
        }
    }

    pub fn kept(&self) -> Option<&Cache> {
        self.kept.as_ref()
    }

    pub fn install(&self) -> Option<&Install> {
        self.install.as_ref()
    }

    pub fn packages(&self) -> Option<&Packages> {
        self.packages.as_deref()
    }

    pub fn downloads_packages(&self) -> bool {
        self.download_packages && self.packages.as_ref().is_some_and(|p| p.downloads())
    }

    pub fn folders(&self) -> &[PathBuf] {
        &self.folders
    }

    pub fn order(&self) -> &[ProviderKind] {
        &self.order
    }

    pub fn cache(&self) -> Option<&Cache> {
        self.cache.as_ref()
    }

    /// Whether asset delivery may use the network, and whether it would
    /// try an Open Cloud key for refused assets.
    pub fn network(&self) -> (bool, bool) {
        match &self.delivery {
            Some(d) => (true, d.may_use_key()),
            None => (false, false),
        }
    }

    /// Resolve a reference, blocking on disk and network. The engine goes
    /// through a [`Loader`](crate::Loader) instead.
    pub fn resolve(&self, r: &ContentRef) -> Result<Fetched, Failure> {
        if let Some(f) = self.failed.lock().expect("failure memo").get(r) {
            return Err(f.clone());
        }
        let result = self.resolve_uncached(r);
        if let Err(f) = &result
            && matches!(f.kind, FailureKind::Network | FailureKind::Offline)
        {
            self.failed
                .lock()
                .expect("failure memo")
                .insert(r.clone(), f.clone());
        }
        result
    }

    /// Resolve with only the local providers (no network, no cache): for
    /// content needed at startup, such as the head mesh.
    pub fn resolve_local(&self, r: &ContentRef) -> Option<Fetched> {
        let (kind, file) = self.locate_local(r)?;
        #[expect(
            clippy::disallowed_methods,
            reason = "the content cache, install lookup and decode workers are rook-platform's known exception (docs/design/architecture.md#crate-layout)"
        )]
        let bytes = std::fs::read(&file)
            .map_err(|e| tracing::warn!(target: "content", "{}: {e}", file.display()))
            .ok()?;
        Some(Fetched::new(bytes, kind, file.display().to_string()))
    }

    /// The local file an `rbxasset://` reference names, without reading it.
    pub fn locate_local(&self, r: &ContentRef) -> Option<(SourceKind, PathBuf)> {
        let ContentRef::File(path) = r else {
            return None;
        };
        self.order.iter().find_map(|kind| {
            let (roots, source): (&[PathBuf], _) = match kind {
                ProviderKind::Install => {
                    let package = self
                        .packages
                        .as_ref()
                        .and_then(|p| p.find(path, self.download_packages));
                    if let Some(file) = package {
                        return Some((SourceKind::Package, file));
                    }
                    (
                        self.install.as_ref().map_or(&[], |i| &i.content),
                        SourceKind::Install,
                    )
                }
                ProviderKind::Folders => (&self.folders, SourceKind::Folder),
                ProviderKind::Overrides | ProviderKind::Defaults | ProviderKind::Delivery => {
                    return None;
                }
            };
            roots
                .iter()
                .find_map(|root| find_file(root, path))
                .map(|file| (source, file))
        })
    }

    fn resolve_uncached(&self, r: &ContentRef) -> Result<Fetched, Failure> {
        let asset = match r {
            ContentRef::Material(material, set, map) => {
                let pack = materials::pack(*material, *set, *map).ok_or_else(|| {
                    Failure::new(FailureKind::NotFound, "no texture for this material")
                })?;
                return self.texture_pack(pack, *map);
            }
            ContentRef::TexturePack(pack, map) => return self.texture_pack(*pack, *map),
            ContentRef::Thumbnail(t) => {
                if !self.order.contains(&ProviderKind::Delivery) {
                    return Err(Failure::new(
                        FailureKind::Offline,
                        "asset delivery isn't in the provider order",
                    ));
                }
                return self.thumbnail(t);
            }
            ContentRef::File(path) => {
                if let Some(bytes) = builtin::find(path) {
                    let name = format!("rbxasset://{path}");
                    return Ok(Fetched::new(bytes.to_vec(), SourceKind::Defaults, name));
                }
                if let Some(found) = self.resolve_local(r) {
                    return Ok(found);
                }
                // A legacy path that now stands for a public asset.
                match defaults::legacy_asset(path) {
                    Some(id) if self.order.contains(&ProviderKind::Defaults) => {
                        ContentRef::Asset { id, version: None }
                    }
                    _ => {
                        let downloaded = self
                            .packages
                            .as_ref()
                            .is_some_and(|p| p.version().is_some());
                        let why = match (downloaded, &self.install) {
                            (false, None) => {
                                "no Roblox install found, and no Roblox packages downloaded"
                            }
                            _ => "no such file in Roblox's packages or install",
                        };
                        return Err(Failure::new(FailureKind::NotFound, why));
                    }
                }
            }
            other => other.clone(),
        };
        if let Some(found) = self.overridden(&asset) {
            return Ok(found);
        }
        if !self.order.contains(&ProviderKind::Delivery) {
            return Err(Failure::new(
                FailureKind::Offline,
                "asset delivery isn't in the provider order",
            ));
        }
        self.download(&asset)
    }

    /// The user's file for an asset id, whatever version is asked for: the
    /// file stands for the asset. One that can't be read is passed over.
    fn overridden(&self, r: &ContentRef) -> Option<Fetched> {
        let ContentRef::Asset { id, .. } = r else {
            return None;
        };
        let file = self.overrides.get(id)?;
        #[expect(
            clippy::disallowed_methods,
            reason = "the content cache, install lookup and decode workers are rook-platform's known exception (docs/design/architecture.md#crate-layout)"
        )]
        let bytes = std::fs::read(file)
            .map_err(|e| tracing::warn!(target: "content", "{}: {e}", file.display()))
            .ok()?;
        Some(Fetched::new(
            bytes,
            SourceKind::Override,
            file.display().to_string(),
        ))
    }
}

/// The cache or kept content folder in `dir`; none (and a warning) if it
/// can't be made.
fn open_store(dir: &Path) -> Option<Cache> {
    Cache::open(dir)
        .map_err(|e| tracing::warn!(target: "content", "{}: {e}", dir.display()))
        .ok()
}

/// A file under `root` by its `rbxasset://` path, ignoring case. Paths
/// that would leave `root` are refused.
fn find_file(root: &Path, path: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = path
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if parts.is_empty() || parts.iter().any(|p| *p == ".." || p.contains(':')) {
        return None;
    }
    let direct: PathBuf = parts.iter().fold(root.to_path_buf(), |p, c| p.join(c));
    if direct.is_file() {
        return Some(direct);
    }
    // Case-sensitive file systems: match each part without case.
    let mut dir = root.to_path_buf();
    for part in parts {
        #[expect(
            clippy::disallowed_methods,
            reason = "the content cache, install lookup and decode workers are rook-platform's known exception (docs/design/architecture.md#crate-layout)"
        )]
        let entry = std::fs::read_dir(&dir).ok()?.flatten().find(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(part))
        })?;
        dir = entry.path();
    }
    dir.is_file().then_some(dir)
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "tests set up files on the machine running them"
)]
mod tests;
