//! Native message boxes for what ends a windowed run: a place that can't
//! be opened, a GPU Rook can't draw with, a crash. Rook has no CoreGui of
//! its own yet to say these in its window, and opened from Explorer it
//! has no console (console_window.rs).

use rook_engine::console;
use rook_lua::MessageType;

/// Log an error that ended the run and show it: what failed as the
/// title, why as the message. The details stay in the log.
pub(crate) fn error(e: &anyhow::Error) {
    console::print_message(MessageType::Error, &format!("{e:#}"));
    let causes: Vec<String> = e.chain().skip(1).map(ToString::to_string).collect();
    if causes.is_empty() {
        show("Rook", &e.to_string());
    } else {
        show(&e.to_string(), &causes.join("\n\n"));
    }
}

pub(crate) fn show(title: &str, text: &str) {
    rfd::MessageDialog::new()
        .set_title(title)
        .set_level(rfd::MessageLevel::Error)
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
