//! Every setting Rook has, resolved once here and handed to the crates
//! below as plain values; no other crate reads the environment
//! (docs/design/data-and-logging.md#settings): the build's
//! ([`crate::build_config`]), the player's ([`crate::user_settings`]), and
//! run-time ones, each declared once in [`table`]. A flag beats the
//! variable, which beats the default; [`Tier::Developer`] settings count
//! only in builds with the `developer-overrides` feature. The Open Cloud
//! key comes from the OS credential store, read only when it can be used
//! ([`api_key`]).

pub mod table;

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::build_config;
use crate::data_folder::{self, DataDir};
use rook_content::resolver::ApiKey;
use rook_content::resolver::cache;
use rook_content::resolver::install::{self, Places};
use rook_content::resolver::packages::Packages;
use rook_content::{Config, ProviderKind};
use rook_platform::endpoints::Endpoints;

pub use table::*;

/// Whether this build takes [`Tier::Developer`] settings (the
/// `developer-overrides` feature). Such a build says so in its window's
/// title and its log's first line.
pub const DEVELOPER_BUILD: bool = cfg!(feature = "developer-overrides");

#[cfg(all(feature = "published", feature = "developer-overrides"))]
compile_error!(
    "`published` with `developer-overrides`: a build for other people must not let flags or \
     variables redirect its web requests or run other code. Build it with `cargo dist`, which \
     leaves `developer-overrides` out."
);

/// The Open Cloud key's name in the OS credential store.
const KEY_NAME: &str = "Open Cloud API key";

/// Who may change a setting at run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Any build.
    Any,
    /// Only builds with developer overrides ([`DEVELOPER_BUILD`]): what
    /// could redirect a published build's traffic or run other code.
    Developer,
}

/// What a setting is when nothing gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unset {
    /// This value, read as a variable's would be.
    Is(&'static str),
    /// No value; what the caller does instead, as the docs say it.
    Means(&'static str),
}

/// A run-time setting, declared once.
#[derive(Debug)]
pub struct Setting {
    /// Its name in docs and warnings.
    pub name: &'static str,
    pub tier: Tier,
    pub unset: Unset,
    pub var: &'static str,
    /// Its flag, without the dashes, for the programs that take it.
    pub flag: Option<&'static str>,
}

impl Setting {
    /// The flag, for clap's `#[arg(long = ...)]`.
    pub const fn long(&self) -> &'static str {
        match self.flag {
            Some(flag) => flag,
            None => panic!("this setting has no flag"),
        }
    }

    /// Whether this build takes the setting.
    pub fn allowed(&self) -> bool {
        self.tier == Tier::Any || DEVELOPER_BUILD
    }

    /// The setting's value: `flag` (what the program parsed from its
    /// flag), else the variable, else [`Self::unset`]'s value; `None` when
    /// that means none. A variable `parse` refuses is skipped with a
    /// warning; a build that doesn't take the setting ignores both, with a
    /// warning.
    pub fn get<T>(&self, flag: Option<T>, parse: impl Fn(&str) -> Result<T, String>) -> Option<T> {
        let var = std::env::var_os(self.var)
            .filter(|v| !v.is_empty())
            .and_then(|value| match value.into_string() {
                Ok(text) => Some(text),
                Err(_) => {
                    warn_once(format!("{} ignored: not valid Unicode", self.var));
                    None
                }
            });
        if !self.allowed() {
            let given = match (&flag, &var) {
                (Some(_), _) => self.flag.map(|f| format!("--{f}")),
                (None, Some(_)) => Some(self.var.to_owned()),
                (None, None) => None,
            };
            if let Some(given) = given {
                warn_once(format!(
                    "{given} ignored: {} is a developer setting, and this build has no developer overrides",
                    self.name
                ));
            }
        } else {
            if flag.is_some() {
                return flag;
            }
            if let Some(text) = var {
                // Never the value itself: the Open Cloud key is one.
                match parse(&text) {
                    Ok(v) => return Some(v),
                    Err(why) => warn_once(format!("{} ignored: {why}", self.var)),
                }
            }
        }
        match self.unset {
            Unset::Is(text) => {
                Some(parse(text).unwrap_or_else(|why| panic!("{}'s default: {why}", self.name)))
            }
            Unset::Means(_) => None,
        }
    }

    pub fn switch(&self, flag: Option<bool>) -> Option<bool> {
        self.get(flag, |s| {
            rook_content::resolver::parse_switch(s).ok_or_else(|| "not on or off".to_owned())
        })
    }

    pub fn text(&self, flag: Option<String>) -> Option<String> {
        self.get(flag, |s| Ok(s.trim().to_owned()))
    }
}

/// Warn once per message; until logging starts (the data folder is found
/// first) the warnings wait for it.
fn warn_once(message: String) {
    let mut warned = WARNED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if warned.said.contains(&message) {
        return;
    }
    if tracing::dispatcher::has_been_set() {
        tracing::warn!(target: "rook", "{message}");
    } else {
        warned.waiting.push(message.clone());
    }
    warned.said.push(message);
}

struct Warned {
    said: Vec<String>,
    waiting: Vec<String>,
}

static WARNED: Mutex<Warned> = Mutex::new(Warned {
    said: Vec::new(),
    waiting: Vec::new(),
});

/// Log the warnings made before logging started; `logging::init` calls it.
pub(crate) fn warn_waiting() {
    let waiting = std::mem::take(
        &mut WARNED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .waiting,
    );
    for message in waiting {
        tracing::warn!(target: "rook", "{message}");
    }
}

/// The download cache's size limit.
pub fn cache_max() -> u64 {
    CACHE_MAX_SIZE
        .get(None, |s| {
            cache::parse_size(s).ok_or_else(|| "not a size (500MB, 2GB)".into())
        })
        .expect("has a default")
}

/// The download cache's folder: `cache` in the data folder. `None` only
/// without a data folder.
pub fn cache_dir() -> Option<PathBuf> {
    CACHE_FOLDER
        .text(None)
        .map(PathBuf::from)
        .or_else(|| DataDir::locate().map(|d| d.cache()))
}

/// The data folder `dataFolder` chooses, if it does.
pub(crate) fn data_folder() -> Option<PathBuf> {
    DATA_FOLDER.text(None).map(PathBuf::from)
}

/// Where Roblox installs live on this system.
pub fn install_places() -> Places {
    Places::from_env(|name| match name {
        "LOCALAPPDATA" => data_folder::local_app_data().map(Into::into),
        _ => std::env::var_os(name).filter(|v| !v.is_empty()),
    })
}

/// The Roblox install the developer chose, if any.
fn chosen_install() -> Option<std::ffi::OsString> {
    ROBLOX_INSTALL.text(None).map(Into::into)
}

/// Where web requests go: the build's `webBase` and `webDomains`, with
/// the `webBase` and `webDomains` settings over them. Read once.
pub fn endpoints() -> std::sync::Arc<Endpoints> {
    static ENDPOINTS: OnceLock<std::sync::Arc<Endpoints>> = OnceLock::new();
    ENDPOINTS
        .get_or_init(|| {
            let base = WEB_BASE.text(None);
            let base = base.as_deref().or(build_config::WEB_BASE);
            let domains = WEB_DOMAINS.get(None, web_domains).unwrap_or_default();
            // The settings' domains come last, so they win.
            let domains = build_config::WEB_DOMAINS
                .iter()
                .copied()
                .chain(domains.iter().map(|(d, url)| (d.as_str(), url.as_str())));
            std::sync::Arc::new(Endpoints::new(base, domains))
        })
        .clone()
}

/// `webDomains`: `users=https://users.example.com,badges=off`.
fn web_domains(text: &str) -> Result<Vec<(String, String)>, String> {
    let pairs: Vec<(String, String)> = text
        .split(',')
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            p.split_once('=')
                .map(|(d, url)| (d.trim().to_owned(), url.trim().to_owned()))
                .ok_or_else(|| format!("{:?} is not domain=URL", p.trim()))
        })
        .collect::<Result<_, _>>()?;
    match pairs
        .iter()
        .find(|(_, url)| !url.eq_ignore_ascii_case("off") && !url.contains("://"))
    {
        Some((domain, _)) => Err(format!("{domain}'s is neither off nor a URL")),
        None => Ok(pairs),
    }
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
    let order = CONTENT_ORDER
        .get(None, |s| {
            s.split(',')
                .map(|n| {
                    ProviderKind::from_name(n.trim())
                        .ok_or_else(|| format!("{:?} is not a provider", n.trim()))
                })
                .collect()
        })
        .expect("has a default");
    let folders: Vec<PathBuf> = CONTENT_FOLDERS
        .get(None, |s| Ok(std::env::split_paths(s).collect()))
        .unwrap_or_default();
    let endpoints = endpoints();
    Config {
        order,
        install: install::newest_from(chosen_install().as_deref(), &install_places()),
        packages: packages(),
        folders: folders.into_iter().filter(|p| p.is_dir()).collect(),
        overrides_dir: DataDir::locate().map(|d| d.overrides()),
        network: ASSET_DELIVERY.switch(None).unwrap_or(network),
        api_key: api_key(&endpoints),
        endpoints: (*endpoints).clone(),
        cache_dir: cache_dir(),
        cache_max: Some(cache_max()),
        kept_dir: DataDir::locate().map(|d| d.assets()),
    }
}

/// Where the Open Cloud key comes from, when asset delivery goes to
/// Roblox: `ROOK_OPEN_CLOUD_API_KEY`, else the OS credential store, read
/// only when a refusal asks for it. A build whose own asset delivery isn't
/// Roblox leaves this code out; a run that sends it elsewhere never reads
/// either.
fn api_key(endpoints: &Endpoints) -> Option<ApiKey> {
    if !build_config::ASSET_DELIVERY_ROBLOX || !endpoints.is_roblox("assetdelivery") {
        return None;
    }
    Some(ApiKey::new(|| {
        OPEN_CLOUD_API_KEY.text(None).or_else(|| {
            match key_store().and_then(|store| store.get(KEY_NAME)) {
                Ok(key) => key,
                Err(e) => {
                    // The store's error, never the key.
                    warn_once(format!("couldn't read the Open Cloud key: {e}"));
                    None
                }
            }
        })
    }))
}

/// Save the Open Cloud key in the OS credential store
/// (`rook key set`).
pub fn save_key(key: &str) -> std::io::Result<()> {
    key_store()?.set(KEY_NAME, key)
}

/// Forget the saved Open Cloud key; whether there was one.
pub fn clear_key() -> std::io::Result<bool> {
    key_store()?.clear(KEY_NAME)
}

/// The store the key is saved in, in a build that can send one.
fn key_store() -> std::io::Result<&'static dyn rook_platform::credentials::Credentials> {
    if !build_config::ASSET_DELIVERY_ROBLOX {
        return Err(std::io::Error::other(
            "this build's asset delivery isn't Roblox, so it never sends an Open Cloud key",
        ));
    }
    Ok(&*rook_platform::get().credentials)
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
            config.overrides_dir = None;
            std::sync::Arc::new(rook_content::Resolver::new(config))
        })
        .clone()
}

/// Roblox's CoreScripts for clients to run, if `coreScripts` turns them on
/// (`flag` is `--core-scripts`): the patch from Roblox's packages
/// (downloaded if `content` downloads), else from the chosen Roblox
/// install or the newest Studio one that has it, read once per run.
/// Problems are logged and give `None`. Only builds with developer
/// overrides have the loader.
pub fn core_scripts(
    flag: Option<bool>,
    content: &rook_content::Resolver,
) -> Option<std::sync::Arc<crate::core_scripts::CorePatch>> {
    let wanted = CORE_SCRIPTS.switch(flag) == Some(true);
    // `DEVELOPER_BUILD` is constant, so a build without developer
    // overrides, where `wanted` is never true, leaves the loader out.
    if !wanted || !DEVELOPER_BUILD {
        return None;
    }
    static PATCH: OnceLock<Option<std::sync::Arc<crate::core_scripts::CorePatch>>> =
        OnceLock::new();
    PATCH
        .get_or_init(|| {
            let packages = content.packages();
            let find = |file: &str| {
                let downloaded = packages.and_then(|p| p.find(file, content.downloads_packages()));
                downloaded.or_else(|| {
                    install::find_from(chosen_install().as_deref(), &install_places())
                        .into_iter()
                        .map(|i| i.root.join("ExtraContent").join(file))
                        .find(|p| p.is_file())
                })
            };
            let Some(patch) = find(crate::core_scripts::PATCH) else {
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
            let server_scripts = find(crate::core_scripts::SERVER_SCRIPTS);
            match crate::core_scripts::CorePatch::load(
                &patch,
                server_scripts.as_deref(),
                flags.as_deref(),
            ) {
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
    THREADS.get(None, |s| {
        s.trim()
            .parse()
            .ok()
            .filter(|&n| n > 0)
            .ok_or_else(|| "not a whole number above 0".into())
    })
}

/// The frame rate cap the settings give over the player's `FramerateCap`,
/// read as it is: -1 at the display's rate, 0 uncapped, else a cap.
/// `uncapped` (`--uncapped`) wins over `framerateCap` (`--framerate-cap`).
pub fn framerate_cap(uncapped: Option<bool>, cap: Option<i32>) -> Option<i32> {
    if UNCAPPED.switch(uncapped) == Some(true) {
        return Some(0);
    }
    FRAMERATE_CAP.get(cap, |s| {
        s.trim().parse().map_err(|_| "not a whole number".into())
    })
}

/// The output device to play through: `audioDevice`, then the player's
/// choice (`saved`, from [`crate::user_settings::PlayerSettings`]); `None`
/// is no sound.
pub fn audio_device(saved: Option<&str>) -> Option<rook_audio::Device> {
    let chosen = AUDIO_DEVICE.text(None);
    match chosen.as_deref().or(saved).map(str::trim) {
        None | Some("") => Some(rook_audio::Device::Default),
        Some(v) if v.eq_ignore_ascii_case("default") || v.eq_ignore_ascii_case("on") => {
            Some(rook_audio::Device::Default)
        }
        Some(v) if v.eq_ignore_ascii_case("off") => None,
        Some(name) => Some(rook_audio::Device::Named(name.to_owned())),
    }
}
