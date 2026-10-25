//! Brings a data folder an older Rook laid out up to date, once: its
//! `settings.json` carried over to the player's settings and removed, a
//! download cache left where the system keeps caches moved into `cache\`,
//! and `content\` split into `packages\` and `assets\`
//! (docs/design/data-and-logging.md#older-data-folders). Each step finds
//! nothing to do on later runs.

use std::path::{Path, PathBuf};

use rook_content::resolver::cache::Cache;
use rook_types::Value;

use super::DataDir;

/// What older Rooks kept in `content\` for themselves; the rest there
/// was Roblox's packages.
const ASSET_FOLDERS: &[&str] = &["blobs", "refs"];

/// Update `data`, with `old_caches` as where older Rooks kept downloads;
/// returns what happened, to log.
pub fn run(data: &DataDir, old_caches: &[PathBuf]) -> Vec<String> {
    let mut report = Vec::new();
    settings_file(data, &mut report);
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

/// `settings.json`'s keys that still have a home: `uncapped` becomes
/// `FramerateCap` 0, `audioDevice` [`crate::user_settings::OUTPUT_DEVICE`],
/// `masterVolume` `MasterVolume`. Every other key is reported as dropped,
/// and the file removed.
fn settings_file(data: &DataDir, report: &mut Vec<String>) {
    let path = data.root.join("settings.json");
    let files = &rook_platform::get().files;
    let Ok(text) = files.read_to_string(&path) else {
        return;
    };
    let parsed = serde_json::from_str::<serde_json::Value>(&super::jsonc::to_json(&text));
    let Ok(serde_json::Value::Object(keys)) = parsed else {
        report.push(format!("{} didn't parse; dropped", path.display()));
        return remove(&path, report);
    };
    let mut values = Vec::new();
    let mut dropped = Vec::new();
    for (key, value) in &keys {
        match (key.as_str(), value) {
            ("uncapped", serde_json::Value::Bool(true)) => {
                values.push(("FramerateCap", Value::Int32(0)));
            }
            ("audioDevice", serde_json::Value::String(device))
                if !device.trim().is_empty() && !device.eq_ignore_ascii_case("default") =>
            {
                values.push((
                    crate::user_settings::OUTPUT_DEVICE,
                    Value::String(device.trim().to_owned()),
                ));
            }
            ("masterVolume", serde_json::Value::Number(n)) => {
                let volume = n.as_f64().unwrap_or(1.0).clamp(0.0, 1.0) as f32;
                values.push(("MasterVolume", Value::Float32(volume)));
            }
            // At their defaults: nothing to carry.
            ("uncapped" | "audioDevice", _) | (_, serde_json::Value::Null) => {}
            _ => dropped.push(key.as_str()),
        }
    }
    match crate::user_settings::carry_over(&data.user_settings(), values) {
        Ok(set) if !set.is_empty() => report.push(format!(
            "settings.json: carried {} over to {}",
            set.join(", "),
            crate::user_settings::FILE
        )),
        Ok(_) => {}
        Err(e) => {
            // Kept, so the next run tries again.
            report.push(format!("settings.json: couldn't carry settings over: {e}"));
            return;
        }
    }
    if !dropped.is_empty() {
        report.push(format!(
            "settings.json: dropped {} (build settings and developer overrides now; see docs/guide/tools.md)",
            dropped.join(", ")
        ));
    }
    remove(&path, report);
}

fn remove(path: &Path, report: &mut Vec<String>) {
    match rook_platform::get().files.remove_file(path) {
        Ok(()) => report.push(format!("removed {}", path.display())),
        Err(e) => report.push(format!("couldn't remove {}: {e}", path.display())),
    }
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
    use crate::user_settings::PlayerSettings;

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
        std::fs::write(
            data.root.join("settings.json"),
            "{\n  // hand-edited\n  \"uncapped\": true,\n  \"audioDevice\": \"Speakers\",\n  \"masterVolume\": 0.5,\n  \"cacheMaxSize\": \"5GB\",\n  \"threads\": null,\n}",
        )
        .unwrap();
        std::fs::write(old_cache.join("blob"), b"cached").unwrap();
        std::fs::write(data.content().join("blobs/ab/abcd"), b"kept").unwrap();
        std::fs::write(data.content().join("fonts/a.ttf"), b"font").unwrap();
        std::fs::write(data.content().join("version.txt"), b"version-1").unwrap();

        let report = run(&data, std::slice::from_ref(&old_cache));
        let report = report.join("\n");
        assert!(report.contains("dropped cacheMaxSize ("), "{report}");
        assert!(!data.root.join("settings.json").exists());
        let player = PlayerSettings::load(Some(&data.user_settings()));
        assert_eq!(player.framerate_cap, 0);
        assert_eq!(player.output_device.as_deref(), Some("Speakers"));
        assert_eq!(player.master_volume, 0.5);
        assert!(!old_cache.exists());
        assert_eq!(std::fs::read(data.cache().join("blob")).unwrap(), b"cached");
        assert!(data.assets().join("blobs/ab/abcd").is_file());
        assert!(data.packages().join("fonts/a.ttf").is_file());
        assert!(data.packages().join("version.txt").is_file());

        // An older Rook run since put files back where it kept them.
        std::fs::create_dir_all(data.content().join("fonts")).unwrap();
        std::fs::write(data.content().join("fonts/b.ttf"), b"font").unwrap();
        std::fs::write(data.content().join("version.txt"), b"version-0").unwrap();
        assert_eq!(run(&data, &[old_cache]), Vec::<String>::new());
        assert!(data.packages().join("fonts/b.ttf").is_file());
        assert_eq!(
            std::fs::read(data.packages().join("version.txt")).unwrap(),
            b"version-1"
        );
        assert!(!data.content().join("fonts").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
