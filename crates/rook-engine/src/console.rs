//! Printing script output to a terminal, colored by message type: blue
//! for print, yellow for warn, red for errors, cyan for info. Colors are
//! dropped automatically when output isn't a terminal or `NO_COLOR` is set.
//!
//! Output normally goes straight to stdout. While a prompt owns the
//! terminal line (the `repl` feature), [`set_output`] routes it there
//! instead, so it prints above the prompt. Every line printed also goes to
//! the run's log file ([`crate::logging`]). Both are written on their own
//! thread ([`writer`]), so printing costs the printing thread little;
//! [`flush`] waits for them.

#[cfg(feature = "repl")]
pub mod repl;
mod writer;

use std::sync::Mutex;

use anstyle::{AnsiColor, Style};
use rook_lua::{LogEntry, MessageType};

pub use writer::flush;

type Output = Box<dyn FnMut(String) + Send>;

static OUTPUT: Mutex<Option<Output>> = Mutex::new(None);

/// Send every printed line to `output` instead of stdout (`None` restores
/// stdout). Lines keep their color codes unless `NO_COLOR` is set.
pub fn set_output(output: Option<Output>) {
    *OUTPUT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = output;
}

/// Print a line to the terminal and the log file, or (`terminal` off)
/// only the log file. The writes happen on the output thread
/// ([`writer`]); only the crash report's tail is kept here, so a crash
/// still shows the last lines.
fn emit(line: String, terminal: bool) {
    crate::logging::crash::remember(&line);
    writer::line(line, terminal);
}

/// Write a line to the terminal or the prompt: on the output thread.
fn show(line: String, no_color: bool) {
    let mut output = OUTPUT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match output.as_mut() {
        Some(f) if no_color => f(anstream::adapter::strip_str(&line).to_string()),
        Some(f) => f(line),
        None => anstream::println!("{line}"),
    }
}

fn style(kind: MessageType) -> Style {
    let color = match kind {
        MessageType::Output => AnsiColor::BrightBlue,
        MessageType::Warning => AnsiColor::Yellow,
        MessageType::Error => AnsiColor::BrightRed,
        MessageType::Info => AnsiColor::Cyan,
    };
    Style::new().fg_color(Some(color.into()))
}

/// Print a message from Rook itself (not a script) in the same layout,
/// e.g. `19:25:03.100  Loaded place.rbxl in 1.20 s  -  Rook`.
pub fn print_message(kind: MessageType, message: &str) {
    print_rook(kind, message, true);
}

/// [`print_message`], or with `terminal` off, only to the log file.
pub(crate) fn print_rook(kind: MessageType, message: &str, terminal: bool) {
    let entry = LogEntry {
        kind,
        message: message.to_owned(),
        time: 0.0,
        wall: std::time::SystemTime::now(),
        source: None,
    };
    print_to("Rook", &entry, terminal);
}

/// Print one entry the way Roblox Studio's Output window shows it:
/// `19:25:03.100  message  -  Server - Script:3`. The time and the context
/// are dimmed; the message is colored by type.
pub fn print_entry(side: &str, entry: &LogEntry) {
    print_to(side, entry, true);
}

fn print_to(side: &str, entry: &LogEntry, terminal: bool) {
    let dim = Style::new().dimmed();
    let s = style(entry.kind);
    let time = chrono::DateTime::<chrono::Local>::from(entry.wall).format("%H:%M:%S%.3f");
    let context = match &entry.source {
        Some(source) => format!("{side} - {source}"),
        None => side.to_owned(),
    };
    let mut lines = entry.message.lines();
    let first = lines.next().unwrap_or("");
    emit(
        format!("{dim}{time}{dim:#}  {s}{first}{s:#}  {dim}-  {context}{dim:#}"),
        terminal,
    );
    // Further lines (an error's stack trace) print as their own entries in
    // the info color, as Studio does.
    let info = style(MessageType::Info);
    for line in lines {
        emit(
            format!("{dim}{time}{dim:#}  {info}{line}{info:#}  {dim}-  {side}{dim:#}"),
            terminal,
        );
    }
}
