//! No console window when the player isn't run from a terminal.
//!
//! The player is a console program, so a terminal waits for it and it can
//! print, read the REPL and answer `--help` there. Its manifest
//! (`rook-player.manifest`) tells Windows 11 24H2 and later not to give it
//! a console when opened from Explorer. Older Windows ignores that and
//! opens one, which this closes again at startup.

/// Close the console window if no other program shares it: Windows made
/// it for the player alone, not a terminal it was run from.
#[allow(unsafe_code)]
pub(crate) fn close_unshared() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
        let mut pids = [0u32; 2];
        // SAFETY: the buffer is valid for its length. With no console the
        // count is 0, and with a terminal it's at least 2 (the shell too).
        let count = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) };
        if count == 1 {
            // SAFETY: no handles to the console are held yet. A failure
            // leaves the window open, as before.
            unsafe {
                FreeConsole();
            }
        }
    }
}
