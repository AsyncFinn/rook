//! Rook's data folder: the player's settings, logs, crash reports, the
//! download cache, content and state between runs
//! (`docs/design/data-and-logging.md`).
//!
//! On Windows everything is under `%LOCALAPPDATA%\Rook`. macOS keeps data
//! in `~/Library/Application Support/Rook` and logs in
//! `~/Library/Logs/Rook`; elsewhere data is under `$XDG_DATA_HOME/rook`
//! and logs and state under `$XDG_STATE_HOME/rook`. The `dataFolder`
//! setting puts all of it under one folder instead, in builds with
//! developer overrides.
//!
//! [`migrate`] brings a folder an older Rook laid out up to date, once
//! (docs/design/data-and-logging.md#older-data-folders).

use std::path::{Path, PathBuf};

use rook_content::resolver::cache::Cache;

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
    /// The `dataFolder` setting, or the platform's folders. `None` only
    /// where the system names no home folder.
    pub fn locate() -> Option<Self> {
        if let Some(dir) = crate::settings::data_folder() {
            return Some(Self::under(dir));
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

    /// The player's own settings (`crate::settings::user_settings`).
    pub fn user_settings(&self) -> PathBuf {
        self.root.join(crate::settings::user_settings::FILE)
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

    /// The user's files for asset ids (`rook_content::resolver`'s
    /// `overrides`). Beside `content`, not in it: that folder is Rook's,
    /// and [`migrate`] sorts what it doesn't know there.
    pub fn overrides(&self) -> PathBuf {
        self.root.join("overrides")
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

/// What older Rooks kept in `content\` for themselves; the rest there
/// was Roblox's packages.
const ASSET_FOLDERS: &[&str] = &["blobs", "refs"];

/// Update `data`, with `old_caches` as where older Rooks kept downloads;
/// returns what happened, to log.
pub fn migrate(data: &DataDir, old_caches: &[PathBuf]) -> Vec<String> {
    let mut report = Vec::new();
    for old in old_caches {
        match Cache::move_legacy(old, &data.cache()) {
            Ok(0) => {}
            Ok(n) => report.push(format!(
                "moved {n} cached downloads from {} into {}",
                old.display(),
                data.cache().display()
            )),
            // What didn't move stays, for the next run to try again.
            Err(e) => report.push(format!("couldn't move the cache {}: {e}", old.display())),
        }
    }
    if let Err(e) = split_content(data) {
        report.push(format!(
            "couldn't sort {} into packages and assets: {e}",
            data.content().display()
        ));
    }
    report
}

/// Where older Rooks kept the download cache: where the system keeps
/// caches (`%TEMP%\Rook\cache` on Windows).
pub fn old_cache_dirs() -> Vec<PathBuf> {
    if cfg!(windows) {
        return vec![std::env::temp_dir().join("Rook").join("cache")];
    }
    directories::ProjectDirs::from("", "", "Rook")
        .map(|d| d.cache_dir().to_path_buf())
        .into_iter()
        .collect()
}

/// Move what older Rooks put straight in `content\` into its two parts.
fn split_content(data: &DataDir) -> std::io::Result<()> {
    let files = &rook_platform::get().files;
    let content = data.content();
    let (packages, assets) = (data.packages(), data.assets());
    let entries = match files.list(&content) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        if entry.path == packages || entry.path == assets {
            continue;
        }
        let name = entry.path.file_name().expect("listed in a folder");
        let into = if ASSET_FOLDERS.iter().any(|f| name == *f) {
            &assets
        } else {
            &packages
        };
        files.create_dir_all(into)?;
        let target = into.join(name);
        // An older Rook run after the split puts its files back here; what
        // the split folders hold already wins.
        if !files.exists(&target) {
            files.rename(&entry.path, &target)?;
        } else if entry.is_file {
            files.remove_file(&entry.path)?;
        } else {
            Cache::move_legacy(&entry.path, &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "tests set up files on the machine running them"
)]
mod tests {
    use super::*;

    #[test]
    fn an_old_folder_is_carried_over_once() {
        let root = std::env::temp_dir()
            .join("rook-engine-tests")
            .join(format!("migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let data = DataDir::under(root.join("data"));
        let old_cache = root.join("temp").join("Rook").join("cache");
        for dir in [&data.root, &old_cache, &data.content().join("blobs/ab")] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::create_dir_all(data.content().join("fonts")).unwrap();
        std::fs::write(old_cache.join("blob"), b"cached").unwrap();
        std::fs::write(data.content().join("blobs/ab/abcd"), b"kept").unwrap();
        std::fs::write(data.content().join("fonts/a.ttf"), b"font").unwrap();
        std::fs::write(data.content().join("version.txt"), b"version-1").unwrap();

        migrate(&data, std::slice::from_ref(&old_cache));
        assert!(!old_cache.exists());
        assert_eq!(std::fs::read(data.cache().join("blob")).unwrap(), b"cached");
        assert!(data.assets().join("blobs/ab/abcd").is_file());
        assert!(data.packages().join("fonts/a.ttf").is_file());
        assert!(data.packages().join("version.txt").is_file());

        // An older Rook run since put files back where it kept them.
        std::fs::create_dir_all(data.content().join("fonts")).unwrap();
        std::fs::write(data.content().join("fonts/b.ttf"), b"font").unwrap();
        std::fs::write(data.content().join("version.txt"), b"version-0").unwrap();
        assert_eq!(migrate(&data, &[old_cache]), Vec::<String>::new());
        assert!(data.packages().join("fonts/b.ttf").is_file());
        assert_eq!(
            std::fs::read(data.packages().join("version.txt")).unwrap(),
            b"version-1"
        );
        assert!(!data.content().join("fonts").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
