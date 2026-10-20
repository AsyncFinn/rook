//! Which providers a [`super::Resolver`] asks, in what order, and where
//! its cache and folders are.

use super::*;

/// The providers, in the order a [`Config`] lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    /// What Rook knows itself ([`defaults`]): the legacy `rbxasset://`
    /// paths that now stand for public assets.
    Defaults,
    /// The user's Roblox install, for `rbxasset://` paths.
    Install,
    /// Folders the user points at (`ROOK_CONTENT_DIRS`), laid out like
    /// the install's `content` folder: downloaded content packages or
    /// local overrides.
    Folders,
    /// Roblox's asset delivery, through the disk cache.
    Delivery,
}

impl ProviderKind {
    pub const ALL: [Self; 4] = [Self::Defaults, Self::Install, Self::Folders, Self::Delivery];

    pub fn name(self) -> &'static str {
        match self {
            Self::Defaults => "defaults",
            Self::Install => "install",
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
    /// Where content Rook itself loads in nearly every place is kept for
    /// good ([`kept`]); `None` keeps it in the cache like the rest.
    pub kept_dir: Option<PathBuf>,
}

/// The default cache folder, after moving a cache left in the old
/// Windows location (`%LOCALAPPDATA%\Rook\cache`) into it, once.
fn default_cache_dir() -> Option<PathBuf> {
    let dir = Cache::default_dir()?;
    if let Some(old) = Cache::legacy_dir() {
        // A failed move leaves the old cache where it is; nothing is lost.
        if let Err(e) = Cache::move_legacy(&old, &dir) {
            tracing::warn!(target: "content", "couldn't move the cache from {}: {e}", old.display());
        }
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

    /// Settings from the environment over `base`: the values a
    /// `ROOK_*` variable doesn't set (the engine fills it from
    /// `settings.json` and each program's defaults).
    pub fn from_env_over(base: Base) -> Self {
        let order = std::env::var(Self::ORDER_VAR)
            .ok()
            .map(|v| v.split(',').filter_map(ProviderKind::from_name).collect())
            .unwrap_or_else(|| ProviderKind::ALL.to_vec());
        let folders = std::env::var_os(Self::FOLDERS_VAR)
            .map(|v| std::env::split_paths(&v).filter(|p| p.is_dir()).collect())
            .unwrap_or_default();
        let network = std::env::var(Self::NETWORK_VAR)
            .ok()
            .and_then(|v| parse_switch(&v))
            .unwrap_or(base.network);
        let install = std::env::var_os(install::INSTALL_VAR).or(base.install);
        Self {
            order,
            install: install::newest_from(install.as_deref()),
            folders,
            network,
            api_key: std::env::var(delivery::API_KEY_VAR)
                .ok()
                .filter(|k| !k.trim().is_empty()),
            cache_dir: default_cache_dir(),
            cache_max: Some(Cache::max_from_env().unwrap_or(base.cache_max)),
            kept_dir: base.kept_dir,
        }
    }

    /// [`Config::from_env_over`] with Rook's defaults: `network` is the
    /// default for `ROOK_ASSET_DELIVERY` (tools that exist to show content
    /// turn it on, headless runs and tests leave it off), and nothing is
    /// kept. Programs go through `rook_engine::settings` instead, which
    /// adds `settings.json` and the kept content folder.
    pub fn from_env(network: bool) -> Self {
        Self::from_env_over(Base {
            network,
            ..Base::default()
        })
    }

    /// No providers but the ones given: for tests.
    pub fn empty() -> Self {
        Self {
            order: ProviderKind::ALL.to_vec(),
            install: None,
            folders: Vec::new(),
            network: false,
            api_key: None,
            cache_dir: None,
            cache_max: None,
            kept_dir: None,
        }
    }
}

/// `on`/`off` (or `1`/`0`, `true`/`false`).
pub fn parse_switch(value: &str) -> Option<bool> {
    let v = value.trim();
    if ["on", "1", "true"]
        .iter()
        .any(|s| v.eq_ignore_ascii_case(s))
    {
        Some(true)
    } else if ["off", "0", "false"]
        .iter()
        .any(|s| v.eq_ignore_ascii_case(s))
    {
        Some(false)
    } else {
        None
    }
}

/// What a [`Config`] holds where no `ROOK_*` variable says otherwise.
#[derive(Debug, Clone)]
pub struct Base {
    /// Whether asset delivery may use the network.
    pub network: bool,
    /// The download cache's size limit.
    pub cache_max: u64,
    /// The Roblox install to use (a version or content folder, or `off`);
    /// `None` searches for the newest.
    pub install: Option<std::ffi::OsString>,
    /// The kept content folder ([`kept`]).
    pub kept_dir: Option<PathBuf>,
}

impl Default for Base {
    fn default() -> Self {
        Self {
            network: false,
            cache_max: cache::DEFAULT_MAX,
            install: None,
            kept_dir: None,
        }
    }
}
