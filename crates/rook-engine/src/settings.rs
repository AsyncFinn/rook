//! Every setting Rook has, resolved once here and handed to the crates
//! below as plain values; no other crate reads the environment. There are
//! three kinds (docs/design/data-and-logging.md#settings): build settings
//! ([`crate::build_config`]), which whoever builds Rook decides; the
//! player's own choices, on `UserGameSettings` ([`crate::user_settings`]);
//! and developer overrides, flags and `ROOK_*` variables that win over the
//! build settings only in builds that allow them ([`developer`]). Logging
//! (`--verbose`, `ROOK_LOG`) and the Open Cloud key work in every build.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::build_config;
use crate::data_folder::{self, DataDir};
use rook_content::resolver::cache;
use rook_content::resolver::install::{self, Places};
use rook_content::resolver::packages::Packages;
use rook_content::{Config, ProviderKind};
use rook_platform::endpoints::Endpoints;

/// The environment variables. All but [`LOG`](env::LOG) and
/// [`OPEN_CLOUD_KEY`](env::OPEN_CLOUD_KEY) are developer overrides.
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
    pub const OPEN_CLOUD_KEY: &str = "ROOK_OPEN_CLOUD_API_KEY";
    /// `default`, `off` or a device's name, over the player's choice.
    pub const AUDIO_DEVICE: &str = "ROOK_AUDIO";
    pub const THREADS: &str = "ROOK_THREADS";
    pub const LOG: &str = "ROOK_LOG";
    /// `on` or `off`.
    pub const CORE_SCRIPTS: &str = "ROOK_CORE_SCRIPTS";
    pub const DATA_FOLDER: &str = "ROOK_DATA_DIR";
    /// `off`, a domain (`example.com`) or a URL; see [`super::endpoints`].
    pub const WEB_BASE: &str = "ROOK_WEB_BASE";
}

/// A developer override, `name` (a flag or variable) given as `value`,
/// kept only in builds that allow them; elsewhere it is as if it weren't
/// given, and a warning says so once.
pub fn developer<T>(name: &'static str, value: Option<T>) -> Option<T> {
    if value.is_some() && !build_config::DEVELOPER_OVERRIDES {
        static WARNED: std::sync::Mutex<Vec<&str>> = std::sync::Mutex::new(Vec::new());
        let mut warned = WARNED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !warned.contains(&name) {
            warned.push(name);
            tracing::warn!(target: "rook", "{name} ignored: this build has no developer overrides");
        }
    }
    gate(build_config::DEVELOPER_OVERRIDES, value)
}

fn gate<T>(allowed: bool, value: Option<T>) -> Option<T> {
    value.filter(|_| allowed)
}

/// A variable that is set to something, in any build.
fn always_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn var(name: &'static str) -> Option<String> {
    developer(name, always_var(name))
}

pub(crate) fn var_os(name: &'static str) -> Option<OsString> {
    developer(name, std::env::var_os(name).filter(|v| !v.is_empty()))
}

fn switch(name: &'static str) -> Option<bool> {
    var(name).and_then(|v| rook_content::resolver::parse_switch(&v))
}

/// The download cache's size limit.
pub fn cache_max() -> u64 {
    var(env::CACHE_MAX_SIZE)
        .and_then(|v| cache::parse_size(&v))
        .unwrap_or(build_config::CACHE_MAX_SIZE)
}

/// The download cache's folder: `cache` in the data folder. `None` only
/// without a data folder.
pub fn cache_dir() -> Option<PathBuf> {
    var_os(env::CACHE_FOLDER)
        .map(PathBuf::from)
        .or_else(|| DataDir::locate().map(|d| d.cache()))
}

/// Where Roblox installs live on this system.
pub fn install_places() -> Places {
    Places::from_env(|name| match name {
        "LOCALAPPDATA" => data_folder::local_app_data().map(Into::into),
        _ => std::env::var_os(name).filter(|v| !v.is_empty()),
    })
}

/// The Roblox install the developer chose, if any.
fn chosen_install() -> Option<OsString> {
    var_os(env::ROBLOX_INSTALL)
}

/// Where web requests go: the build's `webBase` and `webDomains`, with
/// `ROOK_WEB_BASE` over `webBase`. Read once.
pub fn endpoints() -> std::sync::Arc<Endpoints> {
    static ENDPOINTS: OnceLock<std::sync::Arc<Endpoints>> = OnceLock::new();
    ENDPOINTS
        .get_or_init(|| {
            let base = var(env::WEB_BASE);
            let base = base.as_deref().or(build_config::WEB_BASE);
            std::sync::Arc::new(Endpoints::new(
                base,
                build_config::WEB_DOMAINS.iter().copied(),
            ))
        })
        .clone()
}

/// Roblox's client packages in the data folder, shared by every resolver
/// of the run so the live version is asked once; `None` without a data
/// folder.
pub fn packages() -> Option<std::sync::Arc<Packages>> {
    static PACKAGES: OnceLock<Option<std::sync::Arc<Packages>>> = OnceLock::new();
    PACKAGES
        .get_or_init(|| {
            let dir = DataDir::locate()?.packages();
            Some(std::sync::Arc::new(Packages::new(dir, &endpoints())))
        })
        .clone()
}

/// Where content resolves from, with `network` as the program's default
/// for downloads (asset delivery, Roblox's packages).
pub fn content_config(network: bool) -> Config {
    let order = match var(env::CONTENT_ORDER) {
        Some(v) => v.split(',').filter_map(ProviderKind::from_name).collect(),
        None => build_config::CONTENT_ORDER.to_vec(),
    };
    let folders: Vec<PathBuf> = var_os(env::CONTENT_FOLDERS)
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    let delivery = switch(env::ASSET_DELIVERY).or(build_config::ASSET_DELIVERY);
    Config {
        order,
        install: install::newest_from(chosen_install().as_deref(), &install_places()),
        packages: packages(),
        folders: folders.into_iter().filter(|p| p.is_dir()).collect(),
        network: delivery.unwrap_or(network),
        api_key: always_var(env::OPEN_CLOUD_KEY),
        endpoints: (*endpoints()).clone(),
        cache_dir: cache_dir(),
        cache_max: Some(cache_max()),
        kept_dir: DataDir::locate().map(|d| d.assets()),
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

/// Roblox's CoreScripts for clients to run, if the build's `coreScripts`
/// turns them on, or a developer does (`--core-scripts`, given as `flag`,
/// then `ROOK_CORE_SCRIPTS`): the patch from Roblox's packages
/// (downloaded if `content` downloads), else from the chosen Roblox
/// install or the newest Studio one that has it, read once per run.
/// Problems are logged and give `None`.
pub fn core_scripts(
    flag: Option<bool>,
    content: &rook_content::Resolver,
) -> Option<std::sync::Arc<crate::core_scripts::CorePatch>> {
    let wanted = developer("--core-scripts", flag)
        .or_else(|| switch(env::CORE_SCRIPTS))
        .unwrap_or(build_config::CORE_SCRIPTS);
    // The second half is constant, so a build that can't turn them on
    // leaves the loader out.
    if !wanted || (!build_config::CORE_SCRIPTS && !build_config::DEVELOPER_OVERRIDES) {
        return None;
    }
    static PATCH: OnceLock<Option<std::sync::Arc<crate::core_scripts::CorePatch>>> =
        OnceLock::new();
    PATCH
        .get_or_init(|| {
            let packages = content.packages();
            let downloaded = packages
                .and_then(|p| p.find(crate::core_scripts::PATCH, content.downloads_packages()));
            let patch = downloaded.or_else(|| {
                install::find_from(chosen_install().as_deref(), &install_places())
                    .into_iter()
                    .map(|i| i.root.join("ExtraContent").join(crate::core_scripts::PATCH))
                    .find(|p| p.is_file())
            });
            let Some(patch) = patch else {
                let packages = match packages {
                    None => "there is no data folder to download Roblox's packages into",
                    Some(p) if !p.downloads() => {
                        "Roblox's packages are off (the clientsettings or setup web endpoint)"
                    }
                    Some(_) if !content.downloads_packages() => {
                        "this program doesn't download Roblox's packages unless assetDelivery is on"
                    }
                    Some(_) => "Roblox's packages couldn't be downloaded",
                };
                tracing::warn!(
                    "coreScripts: running without Roblox's UI: {packages}, and no Roblox Studio install has {}",
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
    var(env::THREADS)
        .and_then(|n| n.trim().parse().ok())
        .or(build_config::THREADS)
        .filter(|&n| n > 0)
}

/// The output device to play through: `ROOK_AUDIO`, then the player's
/// choice (`saved`, from [`crate::user_settings::PlayerSettings`]); `None`
/// is no sound.
pub fn audio_device(saved: Option<&str>) -> Option<rook_audio::Device> {
    let chosen = var(env::AUDIO_DEVICE);
    match chosen.as_deref().or(saved).map(str::trim) {
        None | Some("") => Some(rook_audio::Device::Default),
        Some(v) if v.eq_ignore_ascii_case("default") || v.eq_ignore_ascii_case("on") => {
            Some(rook_audio::Device::Default)
        }
        Some(v) if v.eq_ignore_ascii_case("off") => None,
        Some(name) => Some(rook_audio::Device::Named(name.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_count_only_where_the_build_allows_them() {
        assert_eq!(gate(false, Some("on")), None);
        assert_eq!(gate(true, Some("on")), Some("on"));
        assert_eq!(gate::<&str>(true, None), None);
    }
}
