//! Rook's own messages, through `tracing`, in the console's line format
//! (`05:38:06.439  Loaded place.rbxl in 0.17 s  -  Rook`), and a log file
//! per run (`docs/design/data-and-logging.md`).
//!
//! The terminal shows `info` and up by default; `--verbose` shows `debug`
//! (every content failure, with its instance and reason), and `--verbose
//! <subsystem>` shows everything one area logs (`content`, `terrain`, ...).
//! The flag beats `ROOK_LOG`, which beats `settings.json`'s `logLevel`.
//! The log file gets `debug` and up whatever the terminal shows, plus
//! every line the terminal printed (script output too), without colours.
//!
//! [`init`] also installs a panic hook: a panic that reaches the program's
//! `main` ([`run`]) leaves a crash report next to the logs.

pub mod crash;
mod files;
mod layer;

use std::io::{LineWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing::Level;

pub use crash::{run, set_context};
pub use files::{cleanup, newest_log};

use crate::data_folder::DataDir;

/// The areas `--verbose <subsystem>` names: each is its crate's events
/// (`rook_content::...`) and events logged with it as their target.
pub const SUBSYSTEMS: &[&str] = &[
    "content", "terrain", "render", "physics", "audio", "lua", "engine", "net", "formats", "scene",
];

/// What the terminal shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    /// Everything at this level and up.
    pub level: Level,
    /// And everything these subsystems log, at any level.
    pub detail: Vec<String>,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            level: Level::INFO,
            detail: Vec::new(),
        }
    }
}

impl Filter {
    /// A `--verbose` value or `ROOK_LOG`: empty for `debug`, else levels
    /// and subsystems, comma-separated. `None` if a part is neither.
    pub fn parse(text: &str, base: Level) -> Option<Self> {
        let mut filter = Self {
            level: base,
            detail: Vec::new(),
        };
        if text.trim().is_empty() {
            filter.level = Level::DEBUG;
            return Some(filter);
        }
        for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            if let Some(level) = parse_level(part) {
                filter.level = level;
            } else if SUBSYSTEMS.contains(&part) {
                filter.detail.push(part.to_owned());
            } else {
                return None;
            }
        }
        Some(filter)
    }

    /// Whether the terminal shows an event.
    pub fn shows(&self, target: &str, level: Level) -> bool {
        if !ours(target) {
            return level <= Level::WARN;
        }
        level <= self.level || self.detail.iter().any(|s| in_subsystem(target, s))
    }
}

fn parse_level(text: &str) -> Option<Level> {
    text.parse().ok()
}

/// Whether an event's target is Rook's own (its crates, or a subsystem
/// name), rather than a library's.
fn ours(target: &str) -> bool {
    target.starts_with("rook") || SUBSYSTEMS.iter().any(|s| in_subsystem(target, s))
}

fn in_subsystem(target: &str, subsystem: &str) -> bool {
    let crate_name = target.split("::").next().unwrap_or(target);
    crate_name == subsystem || crate_name.strip_prefix("rook_") == Some(subsystem)
}

/// Rewrite `--verbose <subsystem>` to `--verbose=<subsystem>` so a bare
/// `--verbose` before a file name doesn't take the file as its value
/// (declare the flag with `require_equals`).
pub fn fix_verbose_args(args: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for arg in args {
        let joins = out.last().is_some_and(|last| last == "--verbose")
            && arg
                .split(',')
                .all(|p| SUBSYSTEMS.contains(&p) || parse_level(p).is_some());
        if joins {
            out.last_mut().expect("last").push_str(&format!("={arg}"));
        } else {
            out.push(arg);
        }
    }
    out
}

/// How a program logs.
#[derive(Debug, Clone)]
pub struct Options {
    /// The program's name, for the log file's name (`rook-player`).
    pub program: &'static str,
    /// `--verbose`'s value: `Some("")` for a bare `--verbose`.
    pub verbose: Option<String>,
    /// Write a log file for this run.
    pub file: bool,
}

/// The run's log file and the last lines for a crash report.
struct Sink {
    file: Option<LineWriter<Box<dyn Write + Send>>>,
    path: Option<PathBuf>,
}

static SINK: Mutex<Sink> = Mutex::new(Sink {
    file: None,
    path: None,
});

/// Write a line, without colours, to the log file and the crash report's
/// tail.
pub(crate) fn record(line: &str) {
    let plain = anstream::adapter::strip_str(line).to_string();
    crash::remember(&plain);
    let mut sink = SINK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(file) = sink.file.as_mut()
        && writeln!(file, "{plain}").is_err()
    {
        // A full disk shouldn't stop the run; the terminal still shows it.
        sink.file = None;
    }
}

/// This run's log file, if it writes one.
pub fn log_path() -> Option<PathBuf> {
    SINK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .path
        .clone()
}

/// Start logging: the terminal filter from `--verbose`, `ROOK_LOG` and
/// `settings.json`, the log file (after cleaning old ones), and the crash
/// hook. Call once, first thing in `main`.
pub fn init(options: &Options) {
    let loaded = crate::settings::get();
    let mut problems: Vec<String> = loaded.problem.iter().cloned().collect();
    let file_level = loaded.settings.log_level.as_deref().and_then(|l| {
        let level = parse_level(l);
        if level.is_none() {
            problems.push(format!("settings.json: unknown logLevel {l:?}"));
        }
        level
    });
    let base = file_level.unwrap_or(Level::INFO);
    let env = std::env::var(crate::settings::env::LOG).ok().and_then(|v| {
        let filter = Filter::parse(&v, base);
        if filter.is_none() {
            problems.push(format!(
                "{}={v:?}: not a level or subsystem",
                crate::settings::env::LOG
            ));
        }
        filter
    });
    let flag = options.verbose.as_deref().and_then(|v| {
        let filter = Filter::parse(v, base);
        if filter.is_none() {
            problems.push(format!(
                "--verbose {v:?}: not a level or one of {}",
                SUBSYSTEMS.join(", ")
            ));
        }
        filter
    });
    let file_filter = file_level.map(|level| Filter {
        level,
        detail: Vec::new(),
    });
    let filter = crate::settings::pick(flag, env, file_filter, Filter::default());

    let data = DataDir::locate();
    if options.file
        && let Some(data) = &data
    {
        open_file(&data.logs, options.program, &mut problems);
    }
    crash::install(options.program, data.map(|d| d.crashes()));

    use tracing_subscriber::layer::SubscriberExt;
    let subscriber = tracing_subscriber::registry().with(layer::ConsoleLayer::new(filter));
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        problems.push("logging was already set up".into());
    }
    for problem in problems {
        tracing::warn!(target: "rook", "{problem}");
    }
}

fn open_file(logs: &Path, program: &str, problems: &mut Vec<String>) {
    let files = &rook_platform::get().files;
    if let Err(e) = files.create_dir_all(logs) {
        problems.push(format!("couldn't make {}: {e}", logs.display()));
        return;
    }
    cleanup(logs);
    cleanup(&logs.join("crashes"));
    let path = logs.join(format!("{program}-{}.log", files::stamp()));
    match files.create(&path) {
        Ok(file) => {
            let mut sink = SINK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sink.file = Some(LineWriter::new(file));
            sink.path = Some(path);
        }
        Err(e) => problems.push(format!("couldn't write {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests;
