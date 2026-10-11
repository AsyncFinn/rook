//! Printing script output to a terminal, coloured by message type: blue
//! for print, yellow for warn, red for errors, cyan for info. Colours are
//! dropped automatically when output isn't a terminal or `NO_COLOR` is set.

use anstyle::{AnsiColor, Style};
use rook_lua::{LogEntry, MessageType};

fn style(kind: MessageType) -> Style {
    let color = match kind {
        MessageType::Output => AnsiColor::BrightBlue,
        MessageType::Warning => AnsiColor::Yellow,
        MessageType::Error => AnsiColor::BrightRed,
        MessageType::Info => AnsiColor::Cyan,
    };
    Style::new().fg_color(Some(color.into()))
}

/// Print one entry as `[side   12.34] message`, coloured.
pub fn print_entry(side: Option<&str>, entry: &LogEntry) {
    let dim = Style::new().dimmed();
    let tag = match side {
        Some(side) => format!("[{side} {:>8.2}]", entry.time),
        None => format!("[{:>8.2}]", entry.time),
    };
    let s = style(entry.kind);
    anstream::println!("{dim}{tag}{dim:#} {s}{}{s:#}", entry.message);
}
