//! The provider chain: a reference goes to each provider in turn until one
//! has it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::cache::{Cache, Cached};
use crate::delivery::{self, Delivery};
use crate::install::{self, Install};
use crate::studio_cache::StudioCache;
use crate::{ContentRef, Failure, FailureKind, Fetched, SourceKind, defaults};

/// The providers, in the order a [`Config`] lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    /// Content Rook ships ([`defaults`]): its own material textures, and
    /// the legacy `rbxasset://fonts/` paths that now stand for public
    /// assets.
    Defaults,
    /// The user's Roblox install, for `rbxasset://` paths.
    Install,
    /// The user's Roblox content cache, for the material textures Studio
    /// downloads ([`StudioCache`]); they're used before Rook's own
    /// whatever the order.
    StudioCache,
    /// Folders the user points at (`ROOK_CONTENT_DIRS`), laid out like
    /// the install's `content` folder: downloaded content packages or
    /// local overrides.
    Folders,
    /// Roblox's asset delivery, through the disk cache.
    Delivery,
}

impl ProviderKind {
    pub const ALL: [Self; 5] = [
        Self::Defaults,
        Self::Install,
        Self::StudioCache,
        Self::Folders,
        Self::Delivery,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Defaults => "defaults",
            Self::Install => "install",
            Self::StudioCache => "studio-cache",
            Self::Folders => "folders",
            Self::Delivery => "delivery",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// Which providers to use and how.
#[derive(Debug, Clone)]
pub struct Config {
    pub order: Vec<ProviderKind>,
    pub install: Option<Install>,
    /// The folder holding Roblox's content cache (`rbx-storage.db`).
    pub roblox_cache: Option<PathBuf>,
    pub folders: Vec<PathBuf>,
    /// Whether asset delivery may use the network. With it off, only the
    /// cache answers asset ids.
    pub network: bool,
    pub api_key: Option<String>,
    /// `None` keeps nothing on disk.
    pub cache_dir: Option<PathBuf>,
    /// The cache's size limit, kept by a trim in the background when the
    /// resolver opens it; `None` never trims.
    pub cache_max: Option<u64>,
}

/// The default cache folder, after moving a cache left in the old
/// Windows location (`%LOCALAPPDATA%\Rook\cache`) into it, once.
fn default_cache_dir() -> Option<PathBuf> {
    let dir = Cache::default_dir()?;
    if let Some(old) = Cache::legacy_dir() {
        // A failed move leaves the old cache where it is; nothing is lost.
        let _ = Cache::move_legacy(&old, &dir);
    }
    Some(dir)
}

impl Config {
    /// Provider order, comma-separated (`folders,install,defaults,delivery`).
    pub const ORDER_VAR: &str = "ROOK_CONTENT_ORDER";
    /// Content folders, separated like `PATH`.
    pub const FOLDERS_VAR: &str = "ROOK_CONTENT_DIRS";
    /// `on` or `off`: whether asset delivery may use the network.
    pub const NETWORK_VAR: &str = "ROOK_ASSET_DELIVERY";

    /// Settings from the environment. `network` is the default for
    /// `ROOK_ASSET_DELIVERY`: tools that exist to show content turn it on,
    /// headless runs and tests leave it off.
    pub fn from_env(network: bool) -> Self {
        let order = std::env::var(Self::ORDER_VAR)
            .ok()
            .map(|v| v.split(',').filter_map(ProviderKind::from_name).collect())
            .unwrap_or_else(|| ProviderKind::ALL.to_vec());
        let folders = std::env::var_os(Self::FOLDERS_VAR)
            .map(|v| std::env::split_paths(&v).filter(|p| p.is_dir()).collect())
            .unwrap_or_default();
        let network = match std::env::var(Self::NETWORK_VAR) {
            Ok(v) if v.eq_ignore_ascii_case("off") || v == "0" => false,
            Ok(v) if v.eq_ignore_ascii_case("on") || v == "1" => true,
            _ => network,
        };
        Self {
            order,
            install: install::newest(),
            roblox_cache: StudioCache::find(None).map(|c| c.root().to_path_buf()),
            folders,
            network,
            api_key: std::env::var(delivery::API_KEY_VAR)
                .ok()
                .filter(|k| !k.trim().is_empty()),
            cache_dir: default_cache_dir(),
            cache_max: Some(Cache::max_from_env()),
        }
    }

    /// No providers but the ones given: for tests.
    pub fn empty() -> Self {
        Self {
            order: ProviderKind::ALL.to_vec(),
            install: None,
            roblox_cache: None,
            folders: Vec::new(),
            network: false,
            api_key: None,
            cache_dir: None,
            cache_max: None,
        }
    }
}

/// Resolves references to bytes. Shared by every DataModel and the
/// [`Loader`](crate::Loader)'s workers.
#[derive(Debug)]
pub struct Resolver {
    order: Vec<ProviderKind>,
    install: Option<Install>,
    studio_cache: Option<StudioCache>,
    folders: Vec<PathBuf>,
    delivery: Option<Delivery>,
    cache: Option<Cache>,
    /// Failures this run that the disk cache doesn't keep (network errors,
    /// asset delivery being off), so they aren't retried every request.
    failed: Mutex<HashMap<ContentRef, Failure>>,
}

impl Resolver {
    pub fn new(config: Config) -> Self {
        Self {
            studio_cache: config
                .roblox_cache
                .as_deref()
                .and_then(|root| StudioCache::at(root, config.cache_dir.clone())),
            order: config.order,
            install: config.install,
            folders: config.folders,
            delivery: config.network.then(|| Delivery::new(config.api_key)),
            cache: config.cache_dir.and_then(|dir| {
                let cache = Cache::open(dir).ok()?;
                if let Some(max) = config.cache_max {
                    // Off the caller's thread: walking a big cache takes a
                    // while, and nothing waits on it. Blobs a resolution is
                    // reading can't be deleted on Windows, and a deleted one
                    // is fetched again.
                    let trimmed = cache.clone();
                    let _ = std::thread::Builder::new()
                        .name("rook-cache-trim".into())
                        .spawn(move || trimmed.trim(max));
                }
                Some(cache)
            }),
            failed: Mutex::new(HashMap::new()),
        }
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
        let bytes = std::fs::read(&file).ok()?;
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
                ProviderKind::Defaults | ProviderKind::StudioCache | ProviderKind::Delivery => {
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
            ContentRef::Material(material, map) => return self.material(*material, *map),
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

    /// A material's texture: Studio's, from the user's content cache, or
    /// else Rook's own. Either comes decoded; the bytes only name it. The
    /// two aren't mixed: once the cache has any of a material's textures,
    /// the ones it lacks (Studio's placeholders, or ones not downloaded)
    /// are left out rather than drawn from Rook's.
    fn material(
        &self,
        material: rook_types::Material,
        map: crate::MaterialMap,
    ) -> Result<Fetched, Failure> {
        let has = |kind| self.order.contains(&kind);
        if has(ProviderKind::StudioCache)
            && let Some(cache) = &self.studio_cache
            && let Some((image, location)) = cache.material(material, map)
        {
            let name = format!("{location} {}x{}", image.width, image.height);
            let mut fetched = Fetched::new(name.as_bytes(), SourceKind::StudioCache, name.clone());
            fetched.image = Some(std::sync::Arc::new(image));
            return Ok(fetched);
        }
        let studio_has_it = has(ProviderKind::StudioCache)
            && self
                .studio_cache
                .as_ref()
                .is_some_and(|c| c.has_material(material));
        if !studio_has_it
            && has(ProviderKind::Defaults)
            && let Some(image) = defaults::material(material, map)
        {
            let name = format!("rook-material://{}/{}", material.name(), map.name());
            let mut fetched = Fetched::new(name.as_bytes(), SourceKind::Defaults, name.clone());
            fetched.image = Some(std::sync::Arc::new(image));
            return Ok(fetched);
        }
        Err(Failure::new(
            FailureKind::NotFound,
            "no texture for this material",
        ))
    }

    /// An asset id, from the cache or asset delivery.
    fn download(&self, r: &ContentRef) -> Result<Fetched, Failure> {
        let key = r.cache_key().expect("downloadable reference");
        match self.cache.as_ref().map(|c| c.get(&key)) {
            Some(Cached::Hit {
                bytes, path, url, ..
            }) => {
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
            Ok((bytes, url)) => {
                let location = match self.cache.as_ref().map(|c| c.put(&key, &bytes, &url)) {
                    Some(Ok(path)) => format!("{url} -> {}", path.display()),
                    _ => url,
                };
                Ok(Fetched::new(bytes, SourceKind::Delivery, location))
            }
            Err(f) => {
                if let Some(cache) = &self.cache {
                    let _ = cache.put_failure(&key, &f);
                }
                Err(f)
            }
        }
    }
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
mod tests {
    use super::*;
    use crate::InstallKind;

    fn resolver_with_install(name: &str) -> (Resolver, PathBuf) {
        let dir = crate::tests::temp_dir(name);
        std::fs::create_dir_all(dir.join("content/Textures")).unwrap();
        std::fs::write(dir.join("content/Textures/Face.png"), b"face").unwrap();
        let mut config = Config::empty();
        config.install = Install::at(&dir, InstallKind::Custom);
        config.cache_dir = Some(dir.join("cache"));
        (Resolver::new(config), dir)
    }

    #[test]
    fn files_come_from_the_install_ignoring_case() {
        let (resolver, _dir) = resolver_with_install("resolve-install");
        let face = resolver
            .resolve(&ContentRef::parse("rbxasset://textures/face.png").unwrap())
            .unwrap();
        assert_eq!(&*face.bytes, b"face");
        assert_eq!(face.source.kind, SourceKind::Install);
        let missing = resolver
            .resolve(&ContentRef::parse("rbxasset://textures/none.png").unwrap())
            .unwrap_err();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert!(
            resolver
                .resolve(&ContentRef::File("../secret".into()))
                .is_err()
        );
    }

    #[test]
    fn folders_come_first_when_ordered_first() {
        let (_, dir) = resolver_with_install("resolve-folders");
        std::fs::create_dir_all(dir.join("override/textures")).unwrap();
        std::fs::write(dir.join("override/textures/face.png"), b"mine").unwrap();
        let mut config = Config::empty();
        config.install = Install::at(&dir, InstallKind::Custom);
        config.folders = vec![dir.join("override")];
        let face = ContentRef::File("textures/face.png".into());
        let resolver = Resolver::new(config.clone());
        assert_eq!(&*resolver.resolve(&face).unwrap().bytes, b"face");
        config.order = vec![ProviderKind::Folders, ProviderKind::Install];
        let resolver = Resolver::new(config);
        let found = resolver.resolve(&face).unwrap();
        assert_eq!(&*found.bytes, b"mine");
        assert_eq!(found.source.kind, SourceKind::Folder);
    }

    #[test]
    fn assets_come_from_the_cache_offline() {
        let (resolver, _dir) = resolver_with_install("resolve-cache");
        let asset = ContentRef::Asset {
            id: 5,
            version: None,
        };
        let offline = resolver.resolve(&asset).unwrap_err();
        assert_eq!(offline.kind, FailureKind::Offline);
        resolver
            .cache()
            .unwrap()
            .put("asset-6", b"cached", "https://cdn/x")
            .unwrap();
        let hit = resolver
            .resolve(&ContentRef::Asset {
                id: 6,
                version: None,
            })
            .unwrap();
        assert_eq!(&*hit.bytes, b"cached");
        assert_eq!(hit.source.kind, SourceKind::Cache);
        // A legacy fonts path stands for its asset.
        resolver
            .cache()
            .unwrap()
            .put("asset-12221462", b"head", "u")
            .unwrap();
        let head = resolver
            .resolve(&ContentRef::parse("rbxasset://fonts/head.mesh").unwrap())
            .unwrap();
        assert_eq!(&*head.bytes, b"head");
    }
}
