//! Rook's data folder: settings, logs, crash reports, kept content and
//! state between runs (`docs/design/data-and-logging.md`).
//!
//! On Windows everything is under `%LOCALAPPDATA%\Rook`. macOS keeps data
//! in `~/Library/Application Support/Rook` and logs in
//! `~/Library/Logs/Rook`; elsewhere data is under `$XDG_DATA_HOME/rook`
//! and logs and state under `$XDG_STATE_HOME/rook`. `ROOK_DATA_DIR` puts
//! all of it under one folder instead. The download cache is not here: it
//! stays where the system may clear it ([`default_cache_dir`]).

use std::path::{Path, PathBuf};

/// Where each part of Rook's data lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDir {
    /// `settings.json` and `content\`.
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
        if let Some(dir) =
            std::env::var_os(crate::settings::env::DATA_FOLDER).filter(|d| !d.is_empty())
        {
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

    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    /// The player's own settings (`crate::user_settings`).
    pub fn user_settings(&self) -> PathBuf {
        self.root.join(crate::user_settings::FILE)
    }

    pub fn crashes(&self) -> PathBuf {
        self.logs.join("crashes")
    }

    /// Content Rook itself loads in nearly every place, kept for good
    /// (`rook_content::resolver::kept`).
    pub fn content(&self) -> PathBuf {
        self.root.join("content")
    }

    pub fn repl_history(&self) -> PathBuf {
        self.state.join("repl-history.txt")
    }

    pub fn recent(&self) -> PathBuf {
        self.state.join("recent.json")
    }
}

/// `%LOCALAPPDATA%` on Windows: the data folder's base, and where Roblox
/// installs and older Rook caches live. Read here and nowhere else.
pub fn local_app_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
}

/// Where the download cache goes when no setting moves it: where the
/// system expects caches, so the system may clear it (`%TEMP%\Rook\cache`
/// on Windows, where Storage Sense and cleanup tools can empty it,
/// `~/Library/Caches/Rook` on macOS, `$XDG_CACHE_HOME/rook` elsewhere).
pub fn default_cache_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return Some(std::env::temp_dir().join("Rook").join("cache"));
    }
    let dirs = directories::ProjectDirs::from("", "", "Rook")?;
    Some(dirs.cache_dir().to_path_buf())
}

/// Where Windows caches lived before 2026-10-13, moved from once.
pub fn legacy_cache_dir() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    Some(local_app_data()?.join("Rook").join("cache"))
}
