//! The disk cache for downloaded content, outside the repository.
//!
//! It lives where the system expects caches, so the system may clear it
//! (`ROOK_CACHE_DIR`, else `%TEMP%\Rook\cache` on Windows, where Storage
//! Sense and cleanup tools can empty it, `~/Library/Caches/Rook` on macOS
//! and `$XDG_CACHE_HOME/rook` elsewhere). Anything cleared is fetched again
//! when needed. Rook keeps it under a size itself ([`Cache::trim`],
//! `ROOK_CACHE_MAX`, 2 GB by default), dropping the blobs used least
//! recently first: reading a blob marks it used. Caches made before
//! 2026-10-13 lived in `%LOCALAPPDATA%\Rook\cache` on Windows and are moved
//! once ([`Cache::move_legacy`]).
//!
//! Layout under the cache folder:
//!
//! - `blobs/<first 2 hex digits>/<blake3 hex>`: content, named by its hash,
//!   so two references to the same bytes share one file.
//! - `refs/<key>`: one small text file per reference (`asset-12221462`,
//!   `asset-5-v3`, `assetversion-77`), either `ok <hash> <size> <unix time>
//!   <url>` or `fail <kind> <unix time> <message>`.
//!
//! Every file is written to a temporary name and renamed into place, so a
//! crash never leaves a partial file under a real name. Blobs are checked
//! against their size and hash when read; a damaged one is deleted and the
//! reference counts as uncached, so it is fetched again. Failures that
//! won't change soon (not found, forbidden) are remembered for
//! [`NEGATIVE_TTL`], so a place with hundreds of broken ids doesn't ask
//! the network again on every run; network errors are never remembered.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{Failure, FailureKind};

/// The environment variable that sets the cache folder.
pub const CACHE_VAR: &str = "ROOK_CACHE_DIR";

/// The environment variable that sets the cache's size limit (`2GB`,
/// `500MB`, `750K`; a bare number is megabytes).
pub const MAX_VAR: &str = "ROOK_CACHE_MAX";

/// The size limit without `ROOK_CACHE_MAX`.
pub const DEFAULT_MAX: u64 = 2 << 30;

/// A blob read again within this long isn't marked used again (saves a
/// file write per read).
const TOUCH_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// How long a "not found" or "forbidden" answer is trusted.
pub const NEGATIVE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// What the cache knows about a reference.
#[derive(Debug, Clone)]
pub enum Cached {
    Hit {
        bytes: Arc<[u8]>,
        hash: blake3::Hash,
        /// The blob's file.
        path: PathBuf,
        /// Where it was downloaded from.
        url: String,
    },
    /// A remembered failure, still within [`NEGATIVE_TTL`].
    Failed(Failure),
    Miss,
}

#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    /// The cache in `dir`, created if needed.
    pub fn open(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(dir.join("blobs"))?;
        std::fs::create_dir_all(dir.join("refs"))?;
        Ok(Self { dir })
    }

    /// `ROOK_CACHE_DIR`, or the platform's cache folder.
    pub fn default_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os(CACHE_VAR) {
            return Some(dir.into());
        }
        let env = |name| std::env::var_os(name).map(PathBuf::from);
        if cfg!(windows) {
            Some(std::env::temp_dir().join("Rook").join("cache"))
        } else if cfg!(target_os = "macos") {
            env("HOME").map(|d| d.join("Library/Caches/Rook"))
        } else {
            env("XDG_CACHE_HOME")
                .or_else(|| env("HOME").map(|h| h.join(".cache")))
                .map(|d| d.join("rook"))
        }
    }

    /// Where Windows caches lived before they moved to `%TEMP%`.
    pub fn legacy_dir() -> Option<PathBuf> {
        if !cfg!(windows) || std::env::var_os(CACHE_VAR).is_some() {
            return None;
        }
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Rook").join("cache"))
    }

    /// Move a cache from `from` into `to`, once: the whole folder when `to`
    /// doesn't exist yet, otherwise file by file, keeping what `to` already
    /// has. Files that can't be renamed (another drive) are copied. `from`
    /// is removed afterwards, with its parent if that is left empty.
    /// Returns how many files moved; a cache with nothing at `from` moves
    /// nothing.
    pub fn move_legacy(from: &Path, to: &Path) -> std::io::Result<usize> {
        if !from.is_dir() || from == to {
            return Ok(0);
        }
        let count = files_under(from).len();
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if !to.exists() && std::fs::rename(from, to).is_ok() {
            remove_if_empty(from.parent());
            return Ok(count);
        }
        let mut moved = 0;
        let mut failed = false;
        for file in files_under(from) {
            let Ok(relative) = file.strip_prefix(from) else {
                continue;
            };
            let target = to.join(relative);
            if target.exists() {
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let ok = std::fs::rename(&file, &target).is_ok()
                || (std::fs::copy(&file, &target).is_ok() && std::fs::remove_file(&file).is_ok());
            if ok {
                moved += 1;
            } else {
                failed = true;
            }
        }
        // Anything that didn't move stays where it is, for the next try.
        if !failed {
            std::fs::remove_dir_all(from)?;
            remove_if_empty(from.parent());
        }
        Ok(moved)
    }

    /// The size limit: `ROOK_CACHE_MAX`, or [`DEFAULT_MAX`].
    pub fn max_from_env() -> u64 {
        std::env::var(MAX_VAR)
            .ok()
            .and_then(|v| parse_size(&v))
            .unwrap_or(DEFAULT_MAX)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every blob: (path, size, when last used).
    fn blobs(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        files_under(&self.dir.join("blobs"))
            .into_iter()
            .filter_map(|path| {
                let meta = std::fs::metadata(&path).ok()?;
                let used = meta.modified().unwrap_or(UNIX_EPOCH);
                Some((path, meta.len(), used))
            })
            .collect()
    }

    /// How many blobs the cache holds, their total size in bytes, and how
    /// many references it knows.
    pub fn stats(&self) -> (usize, u64, usize) {
        let blobs = self.blobs();
        let size = blobs.iter().map(|b| b.1).sum();
        (blobs.len(), size, files_under(&self.dir.join("refs")).len())
    }

    /// Delete the blobs used least recently until the cache holds at most
    /// `max` bytes, then the references they leave behind. Leftover
    /// temporary files older than a day go too. Returns (blobs deleted,
    /// bytes freed).
    pub fn trim(&self, max: u64) -> (usize, u64) {
        let mut blobs = self.blobs();
        let mut total: u64 = blobs.iter().map(|b| b.1).sum();
        blobs.sort_by_key(|b| b.2);
        let mut freed = (0, 0);
        for (path, size, _) in blobs {
            if total <= max {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= size;
                freed = (freed.0 + 1, freed.1 + size);
            }
        }
        self.prune_refs();
        freed
    }

    /// Delete the blobs not used for `age`, then the references they leave
    /// behind. Returns (blobs deleted, bytes freed).
    pub fn prune_older_than(&self, age: Duration) -> (usize, u64) {
        let now = SystemTime::now();
        let mut freed = (0, 0);
        for (path, size, used) in self.blobs() {
            let old = now.duration_since(used).is_ok_and(|a| a > age);
            if old && std::fs::remove_file(&path).is_ok() {
                freed = (freed.0 + 1, freed.1 + size);
            }
        }
        self.prune_refs();
        freed
    }

    /// Empty the cache.
    pub fn clear(&self) -> std::io::Result<()> {
        for sub in ["blobs", "refs"] {
            let dir = self.dir.join(sub);
            if dir.exists() {
                std::fs::remove_dir_all(&dir)?;
            }
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Delete references whose blob is gone, failures past
    /// [`NEGATIVE_TTL`], and temporary files a crash left over a day ago.
    fn prune_refs(&self) {
        let now = SystemTime::now();
        for path in files_under(&self.dir.join("refs")) {
            let stale_tmp = path
                .extension()
                .is_some_and(|e| e.to_string_lossy().starts_with("tmp"))
                && std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| now.duration_since(t).is_ok_and(|a| a > NEGATIVE_TTL));
            let key = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let gone = key.is_some_and(|key| {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    return false;
                };
                match text.split(' ').collect::<Vec<_>>().as_slice() {
                    ["ok", hash, ..] => {
                        blake3::Hash::from_hex(hash).is_ok_and(|h| !self.blob_path(&h).exists())
                    }
                    ["fail", ..] => matches!(self.get_at(&key, now), Cached::Miss),
                    _ => false,
                }
            });
            if stale_tmp || gone {
                let _ = std::fs::remove_file(&path);
            }
        }
        for path in files_under(&self.dir.join("blobs")) {
            let stale_tmp = path
                .extension()
                .is_some_and(|e| e.to_string_lossy().starts_with("tmp"))
                && std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| now.duration_since(t).is_ok_and(|a| a > NEGATIVE_TTL));
            if stale_tmp {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    pub fn blob_path(&self, hash: &blake3::Hash) -> PathBuf {
        let hex = hash.to_hex();
        self.dir.join("blobs").join(&hex[..2]).join(hex.as_str())
    }

    fn ref_path(&self, key: &str) -> PathBuf {
        self.dir.join("refs").join(key)
    }

    /// Look up a reference by its cache key.
    pub fn get(&self, key: &str) -> Cached {
        self.get_at(key, SystemTime::now())
    }

    fn get_at(&self, key: &str, now: SystemTime) -> Cached {
        let Ok(text) = std::fs::read_to_string(self.ref_path(key)) else {
            return Cached::Miss;
        };
        let mut parts = text.trim_end().splitn(5, ' ');
        match parts.next() {
            Some("ok") => {
                let (Some(hash), Some(size), Some(_time)) =
                    (parts.next(), parts.next(), parts.next())
                else {
                    return Cached::Miss;
                };
                let url = parts.next().unwrap_or("").to_owned();
                let (Ok(hash), Ok(size)) = (blake3::Hash::from_hex(hash), size.parse::<u64>())
                else {
                    return Cached::Miss;
                };
                let path = self.blob_path(&hash);
                match std::fs::read(&path) {
                    Ok(bytes) if bytes.len() as u64 == size && blake3::hash(&bytes) == hash => {
                        touch(&path, now);
                        Cached::Hit {
                            bytes: bytes.into(),
                            hash,
                            path,
                            url,
                        }
                    }
                    Ok(_) => {
                        // Damaged or partial: forget it so it's fetched again.
                        let _ = std::fs::remove_file(&path);
                        Cached::Miss
                    }
                    Err(_) => Cached::Miss,
                }
            }
            Some("fail") => {
                let (Some(kind), Some(time)) = (parts.next(), parts.next()) else {
                    return Cached::Miss;
                };
                let rest: Vec<&str> = parts.collect();
                let Some(kind) = FailureKind::from_name(kind) else {
                    return Cached::Miss;
                };
                let when = UNIX_EPOCH + Duration::from_secs(time.parse().unwrap_or(0));
                let fresh = now.duration_since(when).is_ok_and(|age| age < NEGATIVE_TTL);
                if fresh {
                    Cached::Failed(Failure::new(kind, rest.join(" ")))
                } else {
                    Cached::Miss
                }
            }
            _ => Cached::Miss,
        }
    }

    /// Store downloaded bytes for a reference. Returns the blob's path.
    pub fn put(&self, key: &str, bytes: &[u8], url: &str) -> std::io::Result<PathBuf> {
        let hash = blake3::hash(bytes);
        let path = self.blob_path(&hash);
        let stored = std::fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64);
        if !stored {
            std::fs::create_dir_all(path.parent().expect("blob folder"))?;
            write_atomic(&path, bytes)?;
        }
        let line = format!(
            "ok {} {} {} {url}\n",
            hash.to_hex(),
            bytes.len(),
            unix_now()
        );
        write_atomic(&self.ref_path(key), line.as_bytes())?;
        Ok(path)
    }

    /// Remember a failure that won't change soon. Others are ignored.
    pub fn put_failure(&self, key: &str, failure: &Failure) -> std::io::Result<()> {
        if !failure.kind.is_lasting() {
            return Ok(());
        }
        let message = failure.message.replace(['\n', '\r'], " ");
        let line = format!("fail {} {} {message}\n", failure.kind.name(), unix_now());
        write_atomic(&self.ref_path(key), line.as_bytes())
    }

    /// Forget a reference (for `--refresh`).
    pub fn forget(&self, key: &str) {
        let _ = std::fs::remove_file(self.ref_path(key));
    }
}

/// Mark a blob used now (its modified time), unless it was within
/// [`TOUCH_INTERVAL`].
fn touch(path: &Path, now: SystemTime) {
    let recent = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|t| now.duration_since(t).is_ok_and(|a| a < TOUCH_INTERVAL));
    if !recent {
        let _ = std::fs::File::options()
            .write(true)
            .open(path)
            .and_then(|f| f.set_modified(now));
    }
}

/// Every file under `dir`, at any depth.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(entry.path()),
                Ok(_) => out.push(entry.path()),
                Err(_) => {}
            }
        }
    }
    out
}

fn remove_if_empty(dir: Option<&Path>) {
    if let Some(dir) = dir {
        // Fails, harmlessly, when something is left in it.
        let _ = std::fs::remove_dir(dir);
    }
}

/// `2GB`, `500 MB`, `750k`, `1g`: a size in bytes; a bare number is
/// megabytes.
pub fn parse_size(text: &str) -> Option<u64> {
    let t = text.trim().to_ascii_lowercase();
    let t = t.strip_suffix('b').unwrap_or(&t).trim_end();
    let (digits, unit) = match t.char_indices().find(|(_, c)| c.is_ascii_alphabetic()) {
        Some((i, _)) => (t[..i].trim(), &t[i..]),
        None => (t, "m"),
    };
    let scale: u64 = match unit {
        "k" => 1 << 10,
        "m" => 1 << 20,
        "g" => 1 << 30,
        "t" => 1 << 40,
        _ => return None,
    };
    let n: f64 = digits.parse().ok()?;
    (n >= 0.0).then_some((n * scale as f64) as u64)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Write to a unique temporary file beside `path`, then rename it over
/// `path`.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let tmp = path.with_extension(format!(
        "tmp{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_verifies() {
        let cache = Cache::open(crate::tests::temp_dir("cache")).unwrap();
        assert!(matches!(cache.get("asset-1"), Cached::Miss));
        let path = cache
            .put("asset-1", b"mesh bytes", "https://cdn/x")
            .unwrap();
        let Cached::Hit { bytes, url, .. } = cache.get("asset-1") else {
            panic!("not cached");
        };
        assert_eq!(&*bytes, b"mesh bytes");
        assert_eq!(url, "https://cdn/x");
        // Same bytes under another reference share the blob.
        assert_eq!(cache.put("asset-2", b"mesh bytes", "u").unwrap(), path);

        // A damaged blob is dropped and refetched.
        std::fs::write(&path, b"mesh bytez").unwrap();
        assert!(matches!(cache.get("asset-1"), Cached::Miss));
        assert!(!path.exists());
        // A partial one too.
        cache.put("asset-1", b"mesh bytes", "u").unwrap();
        std::fs::write(&path, b"mesh").unwrap();
        assert!(matches!(cache.get("asset-1"), Cached::Miss));
    }

    /// Reading marks a blob used; trimming drops the least recently used
    /// first, with the references left behind.
    #[test]
    fn trims_least_recently_used() {
        let cache = Cache::open(crate::tests::temp_dir("cache-trim")).unwrap();
        let old = SystemTime::now() - Duration::from_secs(10 * 24 * 60 * 60);
        for (key, bytes) in [
            ("asset-1", [1u8; 100]),
            ("asset-2", [2; 100]),
            ("asset-3", [3; 100]),
        ] {
            let path = cache.put(key, &bytes, "u").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(old)
                .unwrap();
        }
        // asset-1 is read, so it's the most recently used.
        assert!(matches!(cache.get("asset-1"), Cached::Hit { .. }));
        assert_eq!(cache.stats(), (3, 300, 3));
        assert_eq!(cache.trim(150).1, 200);
        assert!(matches!(cache.get("asset-1"), Cached::Hit { .. }));
        assert!(matches!(cache.get("asset-2"), Cached::Miss));
        assert_eq!(cache.stats(), (1, 100, 1));
        // Not used for a day: pruned by age.
        assert_eq!(cache.prune_older_than(Duration::from_secs(60)), (0, 0));
        cache.clear().unwrap();
        assert_eq!(cache.stats(), (0, 0, 0));
    }

    /// A legacy cache moves whole, or merges into an existing one.
    #[test]
    fn moves_a_legacy_cache_once() {
        let root = crate::tests::temp_dir("cache-move");
        let old = Cache::open(root.join("old/Rook/cache")).unwrap();
        old.put("asset-1", b"one", "u").unwrap();
        let to = root.join("new/Rook/cache");
        assert_eq!(Cache::move_legacy(&old.dir, &to).unwrap(), 2);
        assert!(!root.join("old/Rook").exists());
        let moved = Cache::open(&to).unwrap();
        assert!(matches!(moved.get("asset-1"), Cached::Hit { .. }));
        // Again into the existing cache: merged, nothing overwritten.
        let again = Cache::open(root.join("old/Rook/cache")).unwrap();
        again.put("asset-2", b"two", "u").unwrap();
        again.put("asset-1", b"other", "u").unwrap();
        Cache::move_legacy(&again.dir, &to).unwrap();
        assert!(matches!(moved.get("asset-2"), Cached::Hit { .. }));
        let Cached::Hit { bytes, .. } = moved.get("asset-1") else {
            panic!("lost");
        };
        assert_eq!(&*bytes, b"one");
        assert_eq!(Cache::move_legacy(&root.join("nothing"), &to).unwrap(), 0);
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size("2GB"), Some(2 << 30));
        assert_eq!(parse_size("500 MB"), Some(500 << 20));
        assert_eq!(parse_size("750k"), Some(750 << 10));
        assert_eq!(parse_size("1.5g"), Some(3 << 29));
        assert_eq!(parse_size("300"), Some(300 << 20));
        assert_eq!(parse_size("lots"), None);
    }

    #[test]
    fn remembers_lasting_failures_for_a_while() {
        let cache = Cache::open(crate::tests::temp_dir("cache-fail")).unwrap();
        let missing = Failure::new(FailureKind::NotFound, "Request asset was not found");
        cache.put_failure("asset-1", &missing).unwrap();
        let Cached::Failed(f) = cache.get("asset-1") else {
            panic!("not remembered");
        };
        assert_eq!(f, missing);
        let later = SystemTime::now() + NEGATIVE_TTL + Duration::from_secs(1);
        assert!(matches!(cache.get_at("asset-1", later), Cached::Miss));
        // Network errors aren't remembered.
        cache
            .put_failure("asset-2", &Failure::new(FailureKind::Network, "timed out"))
            .unwrap();
        assert!(matches!(cache.get("asset-2"), Cached::Miss));
    }
}
