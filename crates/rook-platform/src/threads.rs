//! Long-lived threads of Rook's own (the console's reader). Short work
//! spread over cores goes to [`crate::parallel`] instead.

use std::fmt::Debug;
use std::io;

pub trait Threads: Send + Sync + Debug {
    /// Start a thread named `name` running `body`. Nobody waits for it:
    /// it ends when `body` returns, or with the process.
    fn spawn(&self, name: &str, body: Box<dyn FnOnce() + Send>) -> io::Result<()>;
}

/// The operating system's threads.
#[cfg(feature = "native")]
#[derive(Debug)]
pub struct OsThreads;

#[cfg(feature = "native")]
impl Threads for OsThreads {
    fn spawn(&self, name: &str, body: Box<dyn FnOnce() + Send>) -> io::Result<()> {
        std::thread::Builder::new()
            .name(name.into())
            .spawn(body)
            .map(drop)
    }
}
