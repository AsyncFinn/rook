//! Crash reports: when a panic reaches a program's `main`, a text file in
//! the logs' `crashes` folder keeps the panic, its backtrace, the last
//! lines logged, the versions and the GPU, so a crash in the windowed
//! player isn't lost with its window.
//!
//! The panic hook only notes each panic: Rook catches some on purpose (a
//! decoder that panics on a damaged file), and those aren't crashes. The
//! report is written by [`run`], which wraps `main`, once a panic gets
//! there.

use std::backtrace::Backtrace;
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, PanicHookInfo};
use std::path::PathBuf;
use std::sync::Mutex;

/// Lines logged before a crash that its report keeps.
const TAIL: usize = 200;

struct State {
    program: &'static str,
    dir: Option<PathBuf>,
    /// The last [`TAIL`] lines logged.
    tail: VecDeque<String>,
    /// What the program set (`GPU`).
    context: Vec<(String, String)>,
    /// The latest panic: its thread, message, place and backtrace.
    panic: Option<String>,
}

static STATE: Mutex<State> = Mutex::new(State {
    program: "rook",
    dir: None,
    tail: VecDeque::new(),
    context: Vec::new(),
    panic: None,
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) fn remember(line: &str) {
    let mut state = state();
    if state.tail.len() == TAIL {
        state.tail.pop_front();
    }
    state.tail.push_back(line.to_owned());
}

/// Add a line to any crash report, such as the GPU and its driver.
pub fn set_context(name: &str, value: String) {
    let mut state = state();
    state.context.retain(|(n, _)| n != name);
    state.context.push((name.to_owned(), value));
}

pub(super) fn install(program: &'static str, dir: Option<PathBuf>) {
    {
        let mut state = state();
        state.program = program;
        state.dir = dir;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        note(info);
        previous(info);
    }));
}

fn note(info: &PanicHookInfo<'_>) {
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(std::string::ToString::to_string)
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(not a string)".into());
    let place = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_default();
    let thread = std::thread::current();
    let text = format!(
        "thread '{}' panicked at {place}:\n{message}\n\nBacktrace:\n{}",
        thread.name().unwrap_or("unnamed"),
        Backtrace::force_capture()
    );
    state().panic = Some(text);
}

/// Run a program's `main`; if it panics, write a crash report and exit
/// with code 101 (as a panic would).
pub fn run<T>(main: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(AssertUnwindSafe(main)) {
        Ok(value) => value,
        Err(_) => {
            match write_report() {
                Some(path) => eprintln!("Rook crashed. Crash report: {}", path.display()),
                None => eprintln!("Rook crashed (no crash report folder)"),
            }
            std::process::exit(101);
        }
    }
}

fn write_report() -> Option<PathBuf> {
    let state = state();
    let dir = state.dir.clone()?;
    let files = &rook_platform::get().files;
    files.create_dir_all(&dir).ok()?;
    let mut text = format!(
        "{} {} crashed at {}\n\n{}\n",
        state.program,
        env!("CARGO_PKG_VERSION"),
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        state.panic.as_deref().unwrap_or("(no panic message)")
    );
    text.push_str(&format!(
        "\nSystem:\n  OS: {} {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    for (name, value) in &state.context {
        text.push_str(&format!("  {name}: {value}\n"));
    }
    if let Some(log) = super::log_path() {
        text.push_str(&format!("  Log: {}\n", log.display()));
    }
    text.push_str(&format!("\nLast {} lines logged:\n", state.tail.len()));
    for line in &state.tail {
        text.push_str(line);
        text.push('\n');
    }
    let path = dir.join(format!("{}-{}.txt", state.program, super::files::stamp()));
    files.write(&path, text.as_bytes()).ok()?;
    Some(path)
}
