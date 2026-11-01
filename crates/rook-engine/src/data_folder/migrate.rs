//! Brings a data folder an older Rook laid out up to date, once: a
//! download cache left where the system keeps caches moved into `cache\`,
//! and `content\` split into `packages\` and `assets\`
//! (docs/design/data-and-logging.md#older-data-folders). Each step finds
//! nothing to do on later runs.

use std::path::PathBuf;

use rook_content::resolver::cache::Cache;

use super::DataDir;

/// What older Rooks kept in `content\` for themselves; the rest there
/// was Roblox's packages.
const ASSET_FOLDERS: &[&str] = &["blobs", "refs"];

/// Update `data`, with `old_caches` as where older Rooks kept downloads;
/// returns what happened, to log.
pub fn run(data: &DataDir, old_caches: &[PathBuf]) -> Vec<String> {
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

        run(&data, std::slice::from_ref(&old_cache));
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
