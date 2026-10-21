//! The provider chain: a reference goes to each provider in turn until one
//! has it.

#[expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "the content cache, install lookup and decode workers are rook-platform's known exception (architecture §2)"
)]
pub mod cache;
mod config;
pub mod defaults;
pub mod delivery;
#[expect(
    clippy::disallowed_methods,
    reason = "the content cache, install lookup and decode workers are rook-platform's known exception (architecture §2)"
)]
pub mod install;
pub mod kept;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::materials::{self, TexturePack};
use crate::{ContentRef, Failure, FailureKind, Fetched, MaterialMap, SourceKind};
use cache::{Cache, Cached};
use delivery::Delivery;
use install::Install;

pub use config::{Config, ProviderKind, parse_switch};

/// Resolves references to bytes. Shared by every DataModel and the
/// [`Loader`](crate::Loader)'s workers.
#[derive(Debug)]
pub struct Resolver {
    order: Vec<ProviderKind>,
    install: Option<Install>,
    folders: Vec<PathBuf>,
    delivery: Option<Delivery>,
    cache: Option<Cache>,
    /// Content kept for good ([`kept`]).
    kept: Option<Cache>,
    /// Failures this run that the disk cache doesn't keep (network errors,
    /// asset delivery being off), so they aren't retried every request.
    failed: Mutex<HashMap<ContentRef, Failure>>,
}

impl Resolver {
    pub fn new(config: Config) -> Self {
        Self {
            order: config.order,
            install: config.install,
            folders: config.folders,
            delivery: config.network.then(|| Delivery::new(config.api_key)),
            cache: config.cache_dir.and_then(|dir| {
                let cache = open_store(&dir)?;
                if let Some(max) = config.cache_max {
                    // Off the caller's thread: walking a big cache takes a
                    // while, and nothing waits on it. Blobs a resolution is
                    // reading can't be deleted on Windows, and a deleted one
                    // is fetched again.
                    let trimmed = cache.clone();
                    #[expect(clippy::disallowed_types, reason = "the content cache, install lookup and decode workers are rook-platform's known exception (architecture §2)")]
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
            failed: Mutex::new(HashMap::new()),
        }
    }

    /// The kept content folder ([`kept`]).
    pub fn kept(&self) -> Option<&Cache> {
        self.kept.as_ref()
    }

    pub fn install(&self) -> Option<&Install> {
        self.install.as_ref()
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

    /// Whether asset delivery may use the network, and whether it has an
    /// Open Cloud key.
    pub fn network(&self) -> (bool, bool) {
        match &self.delivery {
            Some(d) => (true, d.has_api_key()),
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
            reason = "the content cache, install lookup and decode workers are rook-platform's known exception (architecture §2)"
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
                ProviderKind::Install => (
                    self.install.as_ref().map_or(&[], |i| &i.content),
                    SourceKind::Install,
                ),
                ProviderKind::Folders => (&self.folders, SourceKind::Folder),
                ProviderKind::Defaults | ProviderKind::Delivery => {
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
            ContentRef::Thumbnail(_) => {
                return Err(Failure::new(
                    FailureKind::Unsupported,
                    "rbxthumb thumbnails aren't resolved yet",
                ));
            }
            ContentRef::File(path) => {
                if let Some(found) = self.resolve_local(r) {
                    return Ok(found);
                }
                // A legacy path that now stands for a public asset.
                match defaults::legacy_asset(path) {
                    Some(id) if self.order.contains(&ProviderKind::Defaults) => {
                        ContentRef::Asset { id, version: None }
                    }
                    _ => {
                        let why = match &self.install {
                            Some(_) => "no such file in the Roblox install",
                            None => "no Roblox install found",
                        };
                        return Err(Failure::new(FailureKind::NotFound, why));
                    }
                }
            }
            other => other.clone(),
        };
        if !self.order.contains(&ProviderKind::Delivery) {
            return Err(Failure::new(
                FailureKind::Offline,
                "asset delivery isn't in the provider order",
            ));
        }
        self.download(&asset)
    }

    /// One map of a TexturePack asset (a material's, [`materials`], or
    /// terrain's): the pack names an image per map, and both come through
    /// the cache and asset delivery like any asset. Comes decoded; the
    /// bytes only name it.
    fn texture_pack(&self, pack_id: u64, map: MaterialMap) -> Result<Fetched, Failure> {
        let not_found = |why: &str| Failure::new(FailureKind::NotFound, why);
        let asset = |id| ContentRef::Asset { id, version: None };
        let pack = self.resolve(&asset(pack_id))?;
        let pack = TexturePack::parse(&pack.bytes)
            .ok_or_else(|| Failure::new(FailureKind::Invalid, "not a TexturePack"))?;
        let main = match map {
            MaterialMap::Color => pack.color,
            MaterialMap::Normal => pack.normal,
            MaterialMap::RoughnessMetalness => pack.roughness,
        }
        .ok_or_else(|| not_found("the material's texture pack has no such map"))?;
        let image = self.resolve(&asset(main))?;
        let extra = |id: Option<u64>| match (map, id) {
            (MaterialMap::RoughnessMetalness, Some(id)) => self.resolve(&asset(id)).map(Some),
            _ => Ok(None),
        };
        let metalness = extra(pack.metalness)?;
        let height = extra(pack.height)?;
        let decoded = materials::build(
            map,
            &image.bytes,
            metalness.as_ref().map(|m| &*m.bytes),
            height.as_ref().map(|h| &*h.bytes),
        )
        .map_err(|e| Failure::new(FailureKind::Invalid, e.to_string()))?;
        let name = format!(
            "{} rbxassetid://{pack_id} {} {}x{}",
            image.source.location,
            map.name(),
            decoded.width,
            decoded.height
        );
        let mut fetched = Fetched::new(name.as_bytes(), image.source.kind, name.clone());
        fetched.image = Some(std::sync::Arc::new(decoded));
        Ok(fetched)
    }

    /// An asset id, from the cache or asset delivery; a kept asset
    /// ([`kept`]) at its pinned version, from the kept folder first.
    fn download(&self, r: &ContentRef) -> Result<Fetched, Failure> {
        let pinned = match r {
            ContentRef::Asset { id, version: None } => {
                kept::version(*id).map(|v| ContentRef::Asset {
                    id: *id,
                    version: Some(v),
                })
            }
            _ => None,
        };
        let r = pinned.as_ref().unwrap_or(r);
        let key = r.cache_key().expect("downloadable reference");
        // Kept content is written to the kept folder; without one it goes
        // to the cache like the rest.
        let keep = self.kept.as_ref().filter(|_| pinned.is_some());
        if let Some(Cached::Hit {
            bytes, path, url, ..
        }) = keep.map(|k| k.get(&key))
        {
            return Ok(Fetched::new(
                bytes,
                SourceKind::Kept,
                format!("{} {url}", path.display()),
            ));
        }
        match self.cache.as_ref().map(|c| c.get(&key)) {
            Some(Cached::Hit {
                bytes, path, url, ..
            }) => {
                if let Some(kept) = keep
                    && let Err(e) = kept.put(&key, &bytes, &url)
                {
                    tracing::warn!(target: "content", "couldn't keep {r}: {e}");
                }
                return Ok(Fetched::new(
                    bytes,
                    SourceKind::Cache,
                    format!("{} {url}", path.display()),
                ));
            }
            // An anonymous refusal doesn't stand once there is a key to try.
            Some(Cached::Failed(f))
                if f.kind == FailureKind::Forbidden
                    && self.delivery.as_ref().is_some_and(Delivery::has_api_key) => {}
            Some(Cached::Failed(f)) => return Err(f),
            Some(Cached::Miss) | None => {}
        }
        let Some(delivery) = &self.delivery else {
            return Err(Failure::new(
                FailureKind::Offline,
                "not cached, and asset delivery is off",
            ));
        };
        let fetched = match r {
            ContentRef::Asset { id, version } => delivery.fetch(*id, *version),
            ContentRef::AssetVersion(_) => Err(Failure::new(
                FailureKind::Unsupported,
                "asset delivery no longer looks assets up by asset version id",
            )),
            _ => unreachable!("only assets are downloaded"),
        };
        match fetched {
            Ok(asset) => {
                let location = match keep.or(self.cache.as_ref()) {
                    Some(store) => match store.put(&key, &asset.bytes, &asset.url) {
                        Ok(path) => format!("{} -> {}", asset.url, path.display()),
                        Err(e) => {
                            tracing::warn!(
                                target: "content",
                                "couldn't save {r} in {}: {e}",
                                store.dir().display()
                            );
                            asset.url
                        }
                    },
                    None => asset.url,
                };
                Ok(Fetched::new(asset.bytes, SourceKind::Delivery, location))
            }
            Err(f) => {
                if let Some(cache) = &self.cache
                    && let Err(e) = cache.put_failure(&key, &f)
                {
                    tracing::debug!(target: "content", "couldn't remember why {r} failed: {e}");
                }
                Err(f)
            }
        }
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
            reason = "the content cache, install lookup and decode workers are rook-platform's known exception (architecture §2)"
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
