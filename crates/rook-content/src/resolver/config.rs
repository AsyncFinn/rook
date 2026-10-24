//! Which providers a [`super::Resolver`] asks, in what order, and where
//! its cache and folders are.

use super::*;
use rook_platform::endpoints::Endpoints;

/// The providers, in the order a [`Config`] lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    /// What Rook knows itself ([`defaults`]): the legacy `rbxasset://`
    /// paths that now stand for public assets.
    Defaults,
    /// Roblox's own files, for `rbxasset://` paths: its packages
    /// ([`packages`]), then the user's Roblox install.
    Install,
    /// Folders the user points at, laid out like
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
    /// Roblox's packages, shared by a run's resolvers.
    pub packages: Option<Arc<Packages>>,
    pub folders: Vec<PathBuf>,
    /// Whether content may come over the network: asset delivery, and
    /// Roblox's packages a path needs. With it off, only the cache answers
    /// asset ids, and only packages already downloaded answer paths.
    pub network: bool,
    /// Sent only to Roblox's own Open Cloud host
    /// ([`Endpoints::is_roblox`]).
    pub api_key: Option<String>,
    /// Where asset delivery's requests go.
    pub endpoints: Endpoints,
    /// `None` keeps nothing on disk.
    pub cache_dir: Option<PathBuf>,
    /// The cache's size limit, kept by a trim in the background when the
    /// resolver opens it; `None` never trims.
    pub cache_max: Option<u64>,
    /// Where content Rook itself loads in nearly every place is kept for
    /// good ([`kept`]); `None` keeps it in the cache like the rest.
    pub kept_dir: Option<PathBuf>,
}

impl Config {
    /// No providers but the ones given: for tests, and the base the
    /// engine fills from its settings.
    pub fn empty() -> Self {
        Self {
            order: ProviderKind::ALL.to_vec(),
            install: None,
            packages: None,
            folders: Vec::new(),
            network: false,
            api_key: None,
            endpoints: Endpoints::default(),
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
