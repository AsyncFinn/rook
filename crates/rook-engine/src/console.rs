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

/// Print one entry, coloured, with a short dimmed `server`/`client` tag
/// when output from several DataModels is mixed.
pub fn print_entry(side: Option<&str>, entry: &LogEntry) {
    let s = style(entry.kind);
    match side {
        Some(side) => {
            let dim = Style::new().dimmed();
            anstream::println!("{dim}{side}{dim:#} {s}{}{s:#}", entry.message);
        }
        None => anstream::println!("{s}{}{s:#}", entry.message),
    }
}
