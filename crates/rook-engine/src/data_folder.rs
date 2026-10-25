//! Rook's data folder: the player's settings, logs, crash reports, the
//! download cache, content and state between runs
//! (`docs/design/data-and-logging.md`).
//!
//! On Windows everything is under `%LOCALAPPDATA%\Rook`. macOS keeps data
//! in `~/Library/Application Support/Rook` and logs in
//! `~/Library/Logs/Rook`; elsewhere data is under `$XDG_DATA_HOME/rook`
//! and logs and state under `$XDG_STATE_HOME/rook`. `ROOK_DATA_DIR` puts
//! all of it under one folder instead, in builds with developer overrides.
//! [`migrate`] brings a folder older Rooks laid out up to date.

mod jsonc;
pub mod migrate;

use std::path::{Path, PathBuf};

/// Where each part of Rook's data lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDir {
    /// The player's settings, `cache\` and `content\`.
    pub root: PathBuf,
    /// A log file per run.
    pub logs: PathBuf,
    /// The REPL's history, recent places, the window's place.
    pub state: PathBuf,
}

impl DataDir {
    /// `ROOK_DATA_DIR`, or the platform's folders. `None` only where the
    /// system names no home folder.
    pub fn locate() -> Option<Self> {
        if let Some(dir) = crate::settings::var_os(crate::settings::env::DATA_FOLDER) {
            return Some(Self::under(PathBuf::from(dir)));
        }
        if cfg!(windows) {
            return Some(Self::under(local_app_data()?.join("Rook")));
        }
        let dirs = directories::ProjectDirs::from("", "", "Rook")?;
        if cfg!(target_os = "macos") {
            let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
            let root = dirs.data_dir().to_path_buf();
            return Some(Self {
                state: root.join("state"),
                root,
                logs: home.join("Library/Logs/Rook"),
            });
        }
        let state = dirs
            .state_dir()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dirs.data_local_dir().join("state"));
        Some(Self {
            root: dirs.data_dir().to_path_buf(),
            logs: state.join("logs"),
            state,
        })
    }

    /// Everything under one folder, laid out as on Windows.
    pub fn under(root: PathBuf) -> Self {
        Self {
            logs: root.join("logs"),
            state: root.join("state"),
            root,
        }
    }

    /// The player's own settings (`crate::user_settings`).
    pub fn user_settings(&self) -> PathBuf {
        self.root.join(crate::user_settings::FILE)
    }

    pub fn crashes(&self) -> PathBuf {
        self.logs.join("crashes")
    }

    /// Downloads, trimmed to the cache's size limit
    /// (`rook_content::resolver::cache`); anything dropped is fetched again.
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// Content kept for good, in two parts: [`DataDir::packages`] and
    /// [`DataDir::assets`].
    pub fn content(&self) -> PathBuf {
        self.root.join("content")
    }

    /// Roblox's client packages, downloaded as needed
    /// (`rook_content::resolver::packages`), laid out like an install.
    pub fn packages(&self) -> PathBuf {
        self.content().join("packages")
    }

    /// What Rook itself loads in nearly every place, pinned to a version
    /// (`rook_content::resolver::kept`), in the cache's layout.
    pub fn assets(&self) -> PathBuf {
        self.content().join("assets")
    }

    pub fn repl_history(&self) -> PathBuf {
        self.state.join("repl-history.txt")
    }

    pub fn recent(&self) -> PathBuf {
        self.state.join("recent.json")
    }
}

/// `%LOCALAPPDATA%` on Windows: the data folder's base, and where Roblox
/// installs live. Read here and nowhere else.
pub fn local_app_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
}
