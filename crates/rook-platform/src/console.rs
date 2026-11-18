//! The console a program runs in: none when it isn't run from a terminal.
//!
//! `rook` and `rook-player` are console programs, so a terminal waits for
//! them and they can print there. Their manifests tell Windows 11 24H2 and
//! later not to give them a console when opened from Explorer. Older
//! Windows ignores that and opens one, which this closes again at startup.

/// Close the console window if no other program shares it: Windows made
/// it for this program alone, not a terminal it was run from.
#[allow(unsafe_code)]
pub fn close_unshared() {
    #[cfg(windows)]
    if sharing_programs() == 1 {
        // SAFETY: no handles to the console are held yet. A failure
        // leaves the window open, as before.
        unsafe {
            windows_sys::Win32::System::Console::FreeConsole();
        }
    }
}

/// Whether this program was run from a terminal, rather than opened from
/// Explorer or started by another program without one. Holds after
/// [`close_unshared`], which only closes a console no terminal shares.
pub fn from_terminal() -> bool {
    #[cfg(windows)]
    {
        sharing_programs() >= 2
    }
    #[cfg(not(windows))]
    {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal()
    }
}

/// How many programs share this program's console: 0 with none, 1 when
/// Windows made it for this program alone, and at least 2 in a terminal
/// (the shell too).
#[cfg(windows)]
#[allow(unsafe_code)]
fn sharing_programs() -> u32 {
    use windows_sys::Win32::System::Console::GetConsoleProcessList;
    let mut pids = [0u32; 2];
    // SAFETY: the buffer is valid for its length; a larger count than it
    // holds is returned without filling it.
    unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) }
}
