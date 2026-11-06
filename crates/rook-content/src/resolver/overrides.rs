//! Files the user puts in the data folder's `overrides` folder, each named
//! after the asset id it answers (`1234567.ogg`), for assets that are gone
//! or that they'd rather replace. Any kind of asset works, since a file is
//! read as the asset's bytes and decoded as usual.
//!
//! The folder is read once, when a resolver is made, so a reference only
//! costs a lookup in the table.

use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

/// Each asset id's file in `dir`, which is made if it's missing, so there
/// is a folder to find. Names are sorted first, so two files for one id
/// give the same winner on every run.
pub(super) fn read(dir: &Path) -> FxHashMap<u64, PathBuf> {
    if let Err(e) = std::fs::create_dir_all(dir) {
        tracing::warn!(target: "content", "{}: {e}", dir.display());
    }
    let mut paths: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries.flatten().map(|e| e.path()).collect(),
        Err(e) => {
            tracing::warn!(target: "content", "{}: {e}", dir.display());
            return FxHashMap::default();
        }
    };
    paths.sort_unstable();
    let mut files = FxHashMap::default();
    for path in paths.into_iter().filter(|p| p.is_file()) {
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok());
        let Some(id) = id else {
            tracing::warn!(
                target: "content",
                "{}: ignored, an override is named after its asset id",
                path.display()
            );
            continue;
        };
        if files.contains_key(&id) {
            tracing::warn!(
                target: "content",
                "{}: ignored, another file already overrides asset {id}",
                path.display()
            );
            continue;
        }
        files.insert(id, path);
    }
    if !files.is_empty() {
        tracing::info!(
            target: "content",
            "Asset overrides from {}: {}",
            dir.display(),
            files.len()
        );
    }
    files
}
