//! Every setting Rook has, in one place: each is resolved once, as the
//! program's command-line flag, then its `ROOK_*` environment variable,
//! then `settings.json` in the data folder ([`crate::data_folder`]), then
//! Rook's default ([`pick`]), and handed to the crates below as plain
//! values. No other crate reads the environment. The table of keys,
//! variables and flags is in `docs/guide/tools.md`.
//!
//! Rook writes `settings.json` once, with every key at its default and a
//! comment saying what it does, when it isn't there. People may edit it,
//! add comments and trailing commas, which are read leniently ([`jsonc`]).
//! Unknown keys are ignored, and a file that doesn't parse is reported
//! and left as it is, with Rook's defaults used instead.

pub mod jsonc;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::data_folder::{self, DataDir};
use rook_content::resolver::cache::{self, Cache};
use rook_content::resolver::install::{self, Places};
use rook_content::{Config, ProviderKind};
use rook_platform::endpoints::Endpoints;

/// The environment variables, one per setting that has one.
pub mod env {
    /// Provider order, comma-separated (`folders,install,defaults,delivery`).
    pub const CONTENT_ORDER: &str = "ROOK_CONTENT_ORDER";
    /// Content folders, separated like `PATH`.
    pub const CONTENT_FOLDERS: &str = "ROOK_CONTENT_DIRS";
    /// `on` or `off`.
    pub const ASSET_DELIVERY: &str = "ROOK_ASSET_DELIVERY";
    pub const CACHE_FOLDER: &str = "ROOK_CACHE_DIR";
    pub const CACHE_MAX_SIZE: &str = "ROOK_CACHE_MAX";
    pub const ROBLOX_INSTALL: &str = "ROOK_ROBLOX_INSTALL";
    /// The key itself; the file setting names a file holding it instead.
    pub const OPEN_CLOUD_KEY: &str = "ROOK_OPEN_CLOUD_API_KEY";
    pub const AUDIO_DEVICE: &str = "ROOK_AUDIO";
    pub const THREADS: &str = "ROOK_THREADS";
    pub const LOG: &str = "ROOK_LOG";
    /// `on` or `off`.
    pub const CORE_SCRIPTS: &str = "ROOK_CORE_SCRIPTS";
    /// Moves the data folder, `settings.json` with it, so it has no key.
    pub const DATA_FOLDER: &str = "ROOK_DATA_DIR";
    /// `off`, a domain (`example.com`) or a URL; see [`super::endpoints`].
    pub const WEB_BASE: &str = "ROOK_WEB_BASE";
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub content_order: Option<Vec<String>>,
    pub content_folders: Option<Vec<PathBuf>>,
    pub asset_delivery: Option<bool>,
    pub roblox_install: Option<String>,
    pub cache_folder: Option<PathBuf>,
    pub cache_max_size: Option<String>,
    pub open_cloud_key_file: Option<PathBuf>,
    pub threads: Option<usize>,
    pub audio_device: Option<String>,
    pub uncapped: Option<bool>,
    pub frame_times: Option<bool>,
    pub log_level: Option<String>,
    pub core_scripts: Option<bool>,
    pub web_base: Option<String>,
    pub web_domains: Option<BTreeMap<String, String>>,
}

/// What Rook writes when there is no file yet: every key at its default,
/// with what it does.
const WRITTEN: &str = r#"{
  // Where content comes from, in order: defaults, install, folders, delivery.
  "contentOrder": ["defaults", "install", "folders", "delivery"],
  // Folders laid out like the Roblox install's `content` folder.
  "contentFolders": [],
  // Whether asset delivery may use the network; null leaves it to each
  // program (on in the player and `rook-tools content`, off in headless runs).
  "assetDelivery": null,
  // The Roblox install to read rbxasset:// files from (a version or content
  // folder, or "off"); null uses the newest one found.
  "robloxInstall": null,
  // The download cache's folder; null puts it where the system keeps caches.
  "cacheFolder": null,
  // The download cache's size limit ("2GB", "500MB"; a bare number is MB).
  "cacheMaxSize": "2GB",
  // A file holding an Open Cloud API key, for assets refused anonymously.
  "openCloudKeyFile": null,
  // Logical cores to plan background threads for; null uses the machine's.
  "threads": null,
  // The output device: "default", "off", or a device's name.
  "audioDevice": "default",
  // Draw as fast as possible instead of waiting for the display.
  "uncapped": false,
  // Print each frame's timings in the terminal.
  "frameTimes": false,
  // What the terminal shows: error, warn, info, debug or trace.
  "logLevel": "info",
  // Run Roblox's own CoreScripts (top bar, player list, chat...) from the
  // Studio install on each player's client. Unfinished, so off by default.
  "coreScripts": false,
  // Where web requests go (asset delivery, users, filtering...): null uses
  // Roblox for asset delivery and answers the rest locally; "off" answers
  // everything locally; "example.com" sends users.roblox.com's requests to
  // users.example.com; a URL ("http://127.0.0.1:8080") sends them to
  // <URL>/users.
  "webBase": null,
  // Per domain, winning over webBase: a base URL, or "off".
  // Example: { "users": "https://users.example.com", "badges": "off" }
  "webDomains": {}
}
"#;

impl Settings {
    /// What [`WRITTEN`] holds.
    pub fn written_default() -> Self {
        Self::parse(WRITTEN).expect("the written settings parse")
    }

    fn parse(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(&jsonc::to_json(text))
    }

    /// Read a settings file leniently. A missing file is no settings; one
    /// that doesn't parse is no settings and the reason.
    pub fn read(path: &Path) -> (Self, Option<String>) {
        let text = match rook_platform::get().files.read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Self::default(), None),
            Err(e) => return (Self::default(), Some(format!("{}: {e}", path.display()))),
        };
        match Self::parse(&text) {
            Ok(settings) => (settings, None),
            Err(e) => (
                Self::default(),
                Some(format!(
                    "{} isn't valid: {e}; using defaults",
                    path.display()
                )),
            ),
        }
    }
}

/// The settings file as read at startup, and what went wrong reading it.
#[derive(Debug)]
pub struct Loaded {
    pub settings: Settings,
    pub problem: Option<String>,
}

/// `settings.json`, read once per run (and written with the defaults if
/// it isn't there).
pub fn get() -> &'static Loaded {
    static LOADED: OnceLock<Loaded> = OnceLock::new();
    LOADED.get_or_init(|| {
        let Some(data) = DataDir::locate() else {
            return Loaded {
                settings: Settings::default(),
                problem: None,
            };
        };
        let path = data.settings();
        let mut problem = None;
        let files = &rook_platform::get().files;
        if !files.exists(&path) {
            let written = files
                .create_dir_all(&data.root)
                .and_then(|()| files.write(&path, WRITTEN.as_bytes()));
            if let Err(e) = written {
                problem = Some(format!("couldn't write {}: {e}", path.display()));
            }
        }
        let (settings, read_problem) = Settings::read(&path);
        Loaded {
            settings,
            problem: read_problem.or(problem),
        }
    })
}

fn file() -> &'static Settings {
    &get().settings
}

/// A value by priority: the command-line flag, then the environment
/// variable, then `settings.json`, then the default.
pub fn pick<T>(flag: Option<T>, env: Option<T>, file: Option<T>, default: T) -> T {
    flag.or(env).or(file).unwrap_or(default)
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn var_os(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|v| !v.is_empty())
}

/// The download cache's size limit.
pub fn cache_max() -> u64 {
    let env = var(env::CACHE_MAX_SIZE).and_then(|v| cache::parse_size(&v));
    let file = file().cache_max_size.as_deref().and_then(cache::parse_size);
    pick(None, env, file, cache::DEFAULT_MAX)
}

/// The download cache's folder; `None` only where the system names no
/// cache folder.
pub fn cache_dir() -> Option<PathBuf> {
    let env = var_os(env::CACHE_FOLDER).map(PathBuf::from);
    env.or_else(|| file().cache_folder.clone())
        .or_else(data_folder::default_cache_dir)
}

/// Where Roblox installs live on this system.
pub fn install_places() -> Places {
    Places::from_env(|name| match name {
        "LOCALAPPDATA" => data_folder::local_app_data().map(Into::into),
        _ => var_os(name),
    })
}

/// The Open Cloud API key: the variable's, or the file the settings name.
fn open_cloud_key() -> Option<String> {
    var(env::OPEN_CLOUD_KEY).or_else(|| {
        let path = file().open_cloud_key_file.as_ref()?;
        match rook_platform::get().files.read_to_string(path) {
            Ok(key) => Some(key.trim().to_owned()).filter(|k| !k.is_empty()),
            Err(e) => {
                tracing::warn!(target: "content", "openCloudKeyFile {}: {e}", path.display());
                None
            }
        }
    })
}

/// Where web requests go: `ROOK_WEB_BASE`, then `webBase`, with
/// `webDomains` winning over either for their domains. Read once.
pub fn endpoints() -> std::sync::Arc<Endpoints> {
    static ENDPOINTS: OnceLock<std::sync::Arc<Endpoints>> = OnceLock::new();
    ENDPOINTS
        .get_or_init(|| {
            let settings = file();
            let base = var(env::WEB_BASE).or_else(|| settings.web_base.clone());
            let domains = settings.web_domains.iter().flatten();
            std::sync::Arc::new(Endpoints::new(
                base.as_deref(),
                domains.map(|(d, url)| (d.as_str(), url.as_str())),
            ))
        })
        .clone()
}

/// Where content resolves from, with `network` as the program's default
/// for asset delivery. Moves a cache left in the old Windows place first.
pub fn content_config(network: bool) -> Config {
    let settings = file();
    let order = var(env::CONTENT_ORDER)
        .map(|v| v.split(',').map(str::to_owned).collect())
        .or_else(|| settings.content_order.clone())
        .map(|names: Vec<String>| {
            names
                .iter()
                .filter_map(|n| ProviderKind::from_name(n))
                .collect()
        })
        .unwrap_or_else(|| ProviderKind::ALL.to_vec());
    let folders: Vec<PathBuf> = match var_os(env::CONTENT_FOLDERS) {
        Some(v) => std::env::split_paths(&v).collect(),
        None => settings.content_folders.clone().unwrap_or_default(),
    };
    let delivery = var(env::ASSET_DELIVERY).and_then(|v| rook_content::resolver::parse_switch(&v));
    let chosen_install =
        var_os(env::ROBLOX_INSTALL).or_else(|| settings.roblox_install.clone().map(Into::into));
    let cache_dir = cache_dir();
    if let (Some(dir), Some(old)) = (&cache_dir, data_folder::legacy_cache_dir())
        && var_os(env::CACHE_FOLDER).is_none()
        && settings.cache_folder.is_none()
        // A failed move leaves the old cache where it is; nothing is lost.
        && let Err(e) = Cache::move_legacy(&old, dir)
    {
        tracing::warn!(target: "content", "couldn't move the cache from {}: {e}", old.display());
    }
    Config {
        order,
        install: install::newest_from(chosen_install.as_deref(), &install_places()),
        folders: folders.into_iter().filter(|p| p.is_dir()).collect(),
        network: pick(None, delivery, settings.asset_delivery, network),
        api_key: open_cloud_key(),
        endpoints: (*endpoints()).clone(),
        cache_dir,
        cache_max: Some(cache_max()),
        kept_dir: DataDir::locate().map(|d| d.content()),
    }
}

/// Content on this machine only (the install and content folders, no
/// cache or network), for what DataModels read at once: GUI fonts. One
/// shared by every DataModel that isn't given its own resolver.
pub fn local_content() -> std::sync::Arc<rook_content::Resolver> {
    static LOCAL: std::sync::OnceLock<std::sync::Arc<rook_content::Resolver>> =
        std::sync::OnceLock::new();
    LOCAL
        .get_or_init(|| {
            let mut config = content_config(false);
            config.network = false;
            config.cache_dir = None;
            config.cache_max = None;
            config.kept_dir = None;
            std::sync::Arc::new(rook_content::Resolver::new(config))
        })
        .clone()
}

/// Roblox's CoreScripts for clients to run, if `--core-scripts`, then
/// `ROOK_CORE_SCRIPTS`, then `coreScripts` turns them on: the patch from
/// the chosen Roblox install (`robloxInstall`) or the newest Studio one
/// that has it, read once per run. Problems are logged and give `None`.
pub fn core_scripts(flag: Option<bool>) -> Option<std::sync::Arc<crate::core_scripts::CorePatch>> {
    let env = var(env::CORE_SCRIPTS).and_then(|v| rook_content::resolver::parse_switch(&v));
    if !pick(flag, env, file().core_scripts, false) {
        return None;
    }
    static PATCH: OnceLock<Option<std::sync::Arc<crate::core_scripts::CorePatch>>> =
        OnceLock::new();
    PATCH
        .get_or_init(|| {
            let chosen = var_os(env::ROBLOX_INSTALL)
                .or_else(|| file().roblox_install.clone().map(Into::into));
            let patch = install::find_from(chosen.as_deref(), &install_places())
                .into_iter()
                .map(|i| i.root.join(crate::core_scripts::PATCH))
                .find(|p| p.is_file());
            let Some(patch) = patch else {
                tracing::warn!(
                    "coreScripts: no Roblox Studio install with {}",
                    crate::core_scripts::PATCH
                );
                return None;
            };
            let flags = data_folder::local_app_data()
                .map(|d| d.join("Roblox/ClientSettings/StudioAppSettings.json"));
            match crate::core_scripts::CorePatch::load(&patch, flags.as_deref()) {
                Ok(p) => Some(std::sync::Arc::new(p)),
                Err(e) => {
                    tracing::warn!("coreScripts: {e}");
                    None
                }
            }
        })
        .clone()
}

/// The logical cores to plan background threads for, if set.
pub fn threads() -> Option<usize> {
    let env = var(env::THREADS).and_then(|n| n.trim().parse().ok());
    env.or(file().threads).filter(|&n| n > 0)
}

/// The output device to play through; `None` is no sound.
pub fn audio_device() -> Option<rook_audio::Device> {
    let chosen = var(env::AUDIO_DEVICE).or_else(|| file().audio_device.clone());
    match chosen.as_deref().map(str::trim) {
        None | Some("") => Some(rook_audio::Device::Default),
        Some(v) if v.eq_ignore_ascii_case("default") || v.eq_ignore_ascii_case("on") => {
            Some(rook_audio::Device::Default)
        }
        Some(v) if v.eq_ignore_ascii_case("off") => None,
        Some(name) => Some(rook_audio::Device::Named(name.to_owned())),
    }
}

/// Whether to draw without waiting for the display: `--uncapped`, then
/// `uncapped`.
pub fn uncapped(flag: bool) -> bool {
    pick(flag.then_some(true), None, file().uncapped, false)
}

/// Whether to print frame timings: `--frame-times`, then `frameTimes`.
pub fn frame_times(flag: bool) -> bool {
    pick(flag.then_some(true), None, file().frame_times, false)
}

#[cfg(test)]
mod tests;
