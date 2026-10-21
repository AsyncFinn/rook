//! The host machine as the portable crates reach it: [`files`], parallel
//! work ([`parallel`]), long-lived [`threads`] and the [`network`].
//!
//! Portable crates never call `std::fs`, `std::thread` or a thread pool
//! themselves (clippy's `disallowed-methods` and cargo-deny keep it so);
//! they ask [`get`]. The app picks the platform once per process: it
//! [`install`]s its own, or turns on the `native` feature, whose
//! [`Platform::native`] is used when nothing was installed. See
//! `docs/design/architecture.md` §2.

pub mod files;
pub mod network;
pub mod parallel;
pub mod threads;

use std::sync::OnceLock;

use files::Files;
use network::Network;
use parallel::Workers;
use threads::Threads;

/// The services of one host.
#[derive(Debug)]
pub struct Platform {
    pub files: Box<dyn Files>,
    pub workers: Box<dyn Workers>,
    pub threads: Box<dyn Threads>,
    pub network: Box<dyn Network>,
}

#[cfg(feature = "native")]
impl Platform {
    /// A desktop's: its file system and threads, Rayon's global pool
    /// (which `rook-physics` sizes) and ureq.
    pub fn native() -> Self {
        Self {
            files: Box::new(files::NativeFiles),
            workers: Box::new(parallel::Rayon),
            threads: Box::new(threads::OsThreads),
            network: Box::new(network::Ureq::new()),
        }
    }
}

static PLATFORM: OnceLock<Platform> = OnceLock::new();

/// Use `platform` for the rest of the process. Fails, handing it back, if
/// one is already in use.
pub fn install(platform: Platform) -> Result<(), Platform> {
    PLATFORM.set(platform)
}

/// The process's platform.
///
/// # Panics
///
/// If nothing was installed and the `native` feature is off: the app
/// forgot to choose one.
pub fn get() -> &'static Platform {
    PLATFORM.get_or_init(unchosen)
}

#[cfg(feature = "native")]
fn unchosen() -> Platform {
    Platform::native()
}

#[cfg(not(feature = "native"))]
fn unchosen() -> Platform {
    panic!("no platform: the app installs one, or turns on rook-platform's `native` feature")
}
