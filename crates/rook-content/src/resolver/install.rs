//! Finding the user's Roblox install, whose `content` folder holds the
//! files `rbxasset://` paths name.
//!
//! Roblox installs each client version in its own folder,
//! `%LOCALAPPDATA%\Roblox\Versions\version-<hash>` (older installers used
//! `Program Files (x86)\Roblox\Versions`), with Studio and the player in
//! separate folders, each holding a `content` folder. On macOS the content
//! is inside the app bundles. Rook uses the newest install it finds,
//! unless the user names one (a version folder or a content folder, or
//! `off` for none).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    Studio,
    Player,
    /// Given by `ROOK_ROBLOX_INSTALL`.
    Custom,
}

/// One Roblox install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    pub kind: InstallKind,
    /// The version folder (or the content folder itself, for a custom one).
    pub root: PathBuf,
    /// Folders `rbxasset://` paths are looked up in, in order: `content`,
    /// then `ExtraContent` and `PlatformContent/pc` when present (a few
    /// `rbxasset://textures/...` paths live in those; Inference from the
    /// archive's paths, see ROADMAP M4 step 2).
    pub content: Vec<PathBuf>,
    /// When it was installed (its executable's modification time).
    pub modified: SystemTime,
}

impl Install {
    /// An install from a version folder (one holding `content`) or a
    /// content folder.
    pub fn at(path: &Path, kind: InstallKind) -> Option<Self> {
        let (root, content) = if path.join("content").is_dir() {
            (path.to_path_buf(), path.join("content"))
        } else if path.is_dir() {
            (path.to_path_buf(), path.to_path_buf())
        } else {
            return None;
        };
        let mut folders = vec![content];
        for extra in ["ExtraContent", "PlatformContent/pc"] {
            let p = root.join(extra);
            if p.is_dir() && root != folders[0] {
                folders.push(p);
            }
        }
        let modified = ["RobloxStudioBeta.exe", "RobloxPlayerBeta.exe"]
            .iter()
            .map(|exe| root.join(exe))
            .chain([root.clone()])
            .find_map(|p| p.metadata().and_then(|m| m.modified()).ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        Some(Self {
            kind,
            root,
            content: folders,
            modified,
        })
    }
}

/// Where Roblox installs live on this system: the folders holding
/// `Roblox\Versions` on Windows, and the application folders on macOS.
#[derive(Debug, Clone, Default)]
pub struct Places {
    pub versions: Vec<PathBuf>,
    pub mac_apps: Vec<PathBuf>,
}

impl Places {
    /// The places for a system whose environment `env` reads: the engine
    /// passes its own lookup, so this crate reads no environment itself.
    pub fn from_env(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Self {
        let versions = ["LOCALAPPDATA", "ProgramFiles(x86)", "ProgramFiles"]
            .iter()
            .filter_map(|name| env(name))
            .map(|base| PathBuf::from(base).join("Roblox").join("Versions"))
            .collect();
        let mut mac_apps = Vec::new();
        if cfg!(target_os = "macos") {
            mac_apps.push(PathBuf::from("/Applications"));
            if let Some(home) = env("HOME") {
                mac_apps.push(PathBuf::from(home).join("Applications"));
            }
        }
        Self { versions, mac_apps }
    }
}

/// Every install found in `places`, newest first, or only the one
/// `chosen` names (a version or content folder; none if it is `off` or
/// doesn't exist).
pub fn find_from(chosen: Option<&std::ffi::OsStr>, places: &Places) -> Vec<Install> {
    if let Some(value) = chosen {
        if value.eq_ignore_ascii_case("off") || value.is_empty() {
            return Vec::new();
        }
        return Install::at(Path::new(&value), InstallKind::Custom)
            .into_iter()
            .collect();
    }
    let mut found = Vec::new();
    for versions in &places.versions {
        let Ok(entries) = std::fs::read_dir(versions) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            let kind = if dir.join("RobloxStudioBeta.exe").is_file() {
                InstallKind::Studio
            } else if dir.join("RobloxPlayerBeta.exe").is_file() {
                InstallKind::Player
            } else {
                continue;
            };
            if dir.join("content").is_dir()
                && let Some(install) = Install::at(&dir, kind)
            {
                found.push(install);
            }
        }
    }
    for (app, kind) in [
        ("RobloxStudio.app", InstallKind::Studio),
        ("Roblox.app", InstallKind::Player),
    ] {
        for apps in &places.mac_apps {
            let resources = apps.join(app).join("Contents/Resources");
            if resources.join("content").is_dir()
                && let Some(install) = Install::at(&resources, kind)
            {
                found.push(install);
            }
        }
    }
    found.sort_by_key(|i| std::cmp::Reverse(i.modified));
    found
}

/// The newest install, or the one `chosen` names ([`find_from`]).
pub fn newest_from(chosen: Option<&std::ffi::OsStr>, places: &Places) -> Option<Install> {
    find_from(chosen, places).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_folder_or_a_content_folder() {
        let dir = crate::tests::temp_dir("install");
        std::fs::create_dir_all(dir.join("v/content/textures")).unwrap();
        std::fs::create_dir_all(dir.join("v/PlatformContent/pc")).unwrap();
        let install = Install::at(&dir.join("v"), InstallKind::Custom).unwrap();
        assert_eq!(
            install.content,
            [dir.join("v/content"), dir.join("v/PlatformContent/pc")]
        );
        let content = Install::at(&dir.join("v/content"), InstallKind::Custom).unwrap();
        assert_eq!(content.content, [dir.join("v/content")]);
        assert!(Install::at(&dir.join("missing"), InstallKind::Custom).is_none());
    }
}
