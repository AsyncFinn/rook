//! The disk cache for downloaded content, outside the repository.
//!
//! Layout under the cache folder (`ROOK_CACHE_DIR`, else
//! `%LOCALAPPDATA%\Rook\cache`, `~/Library/Caches/Rook` or
//! `$XDG_CACHE_HOME/rook`):
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
            env("LOCALAPPDATA").map(|d| d.join("Rook").join("cache"))
        } else if cfg!(target_os = "macos") {
            env("HOME").map(|d| d.join("Library/Caches/Rook"))
        } else {
            env("XDG_CACHE_HOME")
                .or_else(|| env("HOME").map(|h| h.join(".cache")))
                .map(|d| d.join("rook"))
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
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
