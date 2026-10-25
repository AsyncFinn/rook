//! A native message box, only where Rook has no window to say it in: a
//! mistyped flag, a GPU it can't draw with, a crash. Release builds on
//! Windows have no console (`windows_subsystem` in main.rs), so a player
//! would see nothing otherwise; builds with a console only print. All else
//! is said in the window ([`crate::app::notice`]).

use rook_engine::console;
use rook_lua::MessageType;

/// Whether this build prints to a console: release builds on Windows
/// don't.
pub(crate) const CONSOLE: bool = cfg!(any(debug_assertions, not(windows)));

/// Log an error that ended the run, and show it with where the log is.
pub(crate) fn error(e: &anyhow::Error) {
    console::print_message(MessageType::Error, &format!("{e:#}"));
    let mut text = e.to_string();
    for cause in e.chain().skip(1) {
        text.push_str(&format!("\n\n{cause}"));
    }
    if let Some(log) = rook_engine::logging::log_path() {
        text.push_str(&format!("\n\nThe log has more: {}", log.display()));
    }
    show(&text);
}

/// Show what was printed, where nothing was there to print it to.
pub(crate) fn show(text: &str) {
    if CONSOLE {
        return;
    }
    rfd::MessageDialog::new()
        .set_title("Rook")
        .set_level(rfd::MessageLevel::Error)
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
