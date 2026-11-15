//! Where content comes from: the cache, the Roblox install, web
//! endpoints, packages, the Open Cloud key and the CoreScripts these
//! settings choose.

use super::*;

/// The Open Cloud key's name in the OS credential store.
pub(super) const KEY_NAME: &str = "Open Cloud API key";

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
pub(super) fn chosen_install() -> Option<std::ffi::OsString> {
    ROBLOX_INSTALL.text(None).map(Into::into)
}

/// Where web requests go: the build's `webBase` and `webDomains`, with
/// the `webBase` and `webDomains` settings over them. Read once.
pub fn endpoints() -> std::sync::Arc<Endpoints> {
    pub(super) static ENDPOINTS: OnceLock<std::sync::Arc<Endpoints>> = OnceLock::new();
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
pub(super) fn web_domains(text: &str) -> Result<Vec<(String, String)>, String> {
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
    pub(super) static PACKAGES: OnceLock<Option<std::sync::Arc<Packages>>> = OnceLock::new();
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
pub(super) fn api_key(endpoints: &Endpoints) -> Option<ApiKey> {
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
pub(super) fn key_store() -> std::io::Result<&'static dyn rook_platform::credentials::Credentials> {
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
    pub(super) static LOCAL: std::sync::OnceLock<std::sync::Arc<rook_content::Resolver>> =
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
    pub(super) static PATCH: OnceLock<Option<std::sync::Arc<crate::core_scripts::CorePatch>>> =
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
