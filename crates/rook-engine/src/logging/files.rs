//! Log and crash report files: their names, and keeping only the recent
//! ones.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Runs whose logs are kept whatever their age.
const KEEP_RUNS: usize = 30;
/// Logs younger than this are kept however many there are.
const KEEP_AGE: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// The most a folder of logs holds; the oldest go first past it.
const KEEP_BYTES: u64 = 100 << 20;

/// Now, for a file name: `2026-10-18_05-38-06`.
pub(super) fn stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string()
}

/// The files directly in a folder: (path, modified, size).
fn files_in(dir: &Path) -> Vec<(PathBuf, SystemTime, u64)> {
    let Ok(entries) = rook_platform::get().files.list(dir) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter(|e| e.is_file)
        .map(|e| (e.path, e.modified, e.len))
        .collect()
}

/// Which of `files` to delete: those beyond the last [`KEEP_RUNS`] and
/// older than [`KEEP_AGE`], then the oldest until the rest fit in
/// [`KEEP_BYTES`].
fn to_delete(mut files: Vec<(PathBuf, SystemTime, u64)>, now: SystemTime) -> Vec<PathBuf> {
    files.sort_by_key(|f| std::cmp::Reverse(f.1));
    let mut delete = Vec::new();
    let mut total = 0;
    for (i, (path, modified, size)) in files.into_iter().enumerate() {
        let age = now.duration_since(modified).unwrap_or_default();
        if (i >= KEEP_RUNS && age > KEEP_AGE) || total + size > KEEP_BYTES {
            delete.push(path);
        } else {
            total += size;
        }
    }
    delete
}

/// Delete old files from a folder of logs or crash reports.
pub fn cleanup(dir: &Path) {
    for path in to_delete(files_in(dir), SystemTime::now()) {
        if let Err(e) = rook_platform::get().files.remove_file(&path) {
            // Another run's open log, on Windows: next time.
            tracing::debug!(target: "rook", "couldn't delete {}: {e}", path.display());
        }
    }
}

/// The newest log file in a folder.
pub fn newest_log(dir: &Path) -> Option<PathBuf> {
    files_in(dir)
        .into_iter()
        .filter(|f| f.0.extension().is_some_and(|e| e == "log"))
        .max_by_key(|f| f.1)
        .map(|f| f.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_recent_runs_young_logs_and_100_mb() {
        let now = SystemTime::now();
        let day = Duration::from_secs(24 * 60 * 60);
        let file =
            |i: u32, age: Duration, size: u64| (PathBuf::from(format!("{i}")), now - age, size);
        // 40 small logs a day apart: the newest 30 stay, and the rest
        // stay while under 14 days old.
        let daily: Vec<_> = (0..40).map(|i| file(i, day * i, 1)).collect();
        let deleted = to_delete(daily, now);
        assert_eq!(deleted.len(), 10);
        assert!(
            deleted
                .iter()
                .all(|p| p.to_str().unwrap().parse::<u32>().unwrap() >= 30)
        );
        // Three 40 MB logs today: the oldest goes.
        let big: Vec<_> = (0..3)
            .map(|i| file(i, Duration::from_secs(u64::from(i)), 40 << 20))
            .collect();
        assert_eq!(to_delete(big, now), [PathBuf::from("2")]);
    }
}
