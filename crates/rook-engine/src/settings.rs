//! `settings.json` in the data folder ([`crate::data`]): the download
//! cache's size, whether asset delivery is on, the Roblox install to use
//! and the default log level.
//!
//! Rook writes it once, as plain pretty JSON, when it isn't there; people
//! may add comments and trailing commas, which are read leniently
//! ([`jsonc`]). Unknown keys are ignored, and a file that doesn't parse
//! is reported and left as it is, with Rook's defaults used instead.
//!
//! Each value comes from, in order: the program's command-line flag, the
//! `ROOK_*` environment variable, `settings.json`, Rook's default
//! ([`pick`]).

pub mod jsonc;

use std::path::Path;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::data_folder::DataDir;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// The download cache's size limit (`2GB`, `500MB`; a bare number is
    /// megabytes). `ROOK_CACHE_MAX` overrides it.
    pub cache_max_size: Option<String>,
    /// Whether asset delivery may use the network; `null` leaves it to
    /// each program (on in the player and `rook-tools content`, off in headless
    /// runs). `ROOK_ASSET_DELIVERY` overrides it.
    pub asset_delivery: Option<bool>,
    /// The Roblox install to read `rbxasset://` files from (a version or
    /// content folder, or `off`); `null` uses the newest one found.
    /// `ROOK_ROBLOX_INSTALL` overrides it.
    pub roblox_install: Option<String>,
    /// What the terminal shows: `error`, `warn`, `info` (the default),
    /// `debug` or `trace`. `--verbose` and `ROOK_LOG` override it.
    pub log_level: Option<String>,
}

impl Settings {
    /// What Rook writes when there is no file yet.
    pub fn written_default() -> Self {
        Self {
            cache_max_size: Some("2GB".into()),
            asset_delivery: None,
            roblox_install: None,
            log_level: Some("info".into()),
        }
    }

    /// Read a settings file leniently. A missing file is no settings; one
    /// that doesn't parse is no settings and the reason.
    pub fn read(path: &Path) -> (Self, Option<String>) {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Self::default(), None),
            Err(e) => return (Self::default(), Some(format!("{}: {e}", path.display()))),
        };
        match serde_json::from_str(&jsonc::to_json(&text)) {
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
        if !path.exists() {
            let text = serde_json::to_string_pretty(&Settings::written_default())
                .expect("settings serialize");
            let written = std::fs::create_dir_all(&data.root)
                .and_then(|()| std::fs::write(&path, text + "\n"));
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

/// A value by priority: the command-line flag, then the environment
/// variable, then `settings.json`, then the default.
pub fn pick<T>(flag: Option<T>, env: Option<T>, file: Option<T>, default: T) -> T {
    flag.or(env).or(file).unwrap_or(default)
}

/// The download cache's size limit: `ROOK_CACHE_MAX`, `cacheMaxSize`, or
/// 2 GB.
pub fn cache_max() -> u64 {
    use rook_content::resolver::cache;
    let file = get()
        .settings
        .cache_max_size
        .as_deref()
        .and_then(cache::parse_size);
    pick(
        None,
        rook_content::Cache::max_from_env(),
        file,
        cache::DEFAULT_MAX,
    )
}

/// Where content resolves: the `ROOK_*` variables over `settings.json`
/// over Rook's defaults, with `network` as the program's default for asset
/// delivery, and the data folder's kept content. Flags are applied by the
/// caller on the result.
pub fn content_config(network: bool) -> rook_content::Config {
    let settings = &get().settings;
    rook_content::Config::from_env_over(rook_content::resolver::Base {
        network: settings.asset_delivery.unwrap_or(network),
        cache_max: cache_max(),
        install: settings.roblox_install.clone().map(Into::into),
        kept_dir: DataDir::locate().map(|d| d.content()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_beats_env_beats_file_beats_default() {
        assert_eq!(pick(Some(1), Some(2), Some(3), 4), 1);
        assert_eq!(pick(None, Some(2), Some(3), 4), 2);
        assert_eq!(pick(None, None, Some(3), 4), 3);
        assert_eq!(pick(None, None, None, 4), 4);
    }

    #[test]
    fn reads_what_it_writes_and_hand_edits() {
        let text = serde_json::to_string_pretty(&Settings::written_default()).unwrap();
        assert_eq!(
            serde_json::from_str::<Settings>(&text).unwrap(),
            Settings::written_default()
        );
        let edited =
            "{\n  // more room\n  \"cacheMaxSize\": \"5GB\",\n  \"assetDelivery\": false,\n}";
        let settings: Settings = serde_json::from_str(&jsonc::to_json(edited)).unwrap();
        assert_eq!(settings.cache_max_size.as_deref(), Some("5GB"));
        assert_eq!(settings.asset_delivery, Some(false));
    }
}
