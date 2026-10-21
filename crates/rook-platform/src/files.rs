//! Files: settings, logs, crash reports and the console's history.

use std::fmt::Debug;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// One thing in a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub is_file: bool,
    /// The Unix epoch where the host doesn't say.
    pub modified: SystemTime,
    pub len: u64,
}

/// A file system. Errors carry no path: callers add the one they asked
/// about.
pub trait Files: Send + Sync + Debug {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    /// Replace a file's contents, making the file if needed.
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    /// A new, empty file to write as it goes (a log).
    fn create(&self, path: &Path) -> io::Result<Box<dyn Write + Send>>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// What's directly in a folder; entries the host can't describe are
    /// left out.
    fn list(&self, dir: &Path) -> io::Result<Vec<Entry>>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
}

impl dyn Files {
    /// A UTF-8 file, as `std::fs::read_to_string` reads it.
    pub fn read_to_string(&self, path: &Path) -> io::Result<String> {
        String::from_utf8(self.read(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

/// The machine's own file system.
#[cfg(feature = "native")]
#[derive(Debug)]
pub struct NativeFiles;

#[cfg(feature = "native")]
impl Files for NativeFiles {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        std::fs::write(path, bytes)
    }

    fn create(&self, path: &Path) -> io::Result<Box<dyn Write + Send>> {
        Ok(Box::new(std::fs::File::create(path)?))
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<Entry>> {
        Ok(std::fs::read_dir(dir)?
            .flatten()
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                Some(Entry {
                    path: e.path(),
                    is_file: meta.is_file(),
                    modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    len: meta.len(),
                })
            })
            .collect())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}
