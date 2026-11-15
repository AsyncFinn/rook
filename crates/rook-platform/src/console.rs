//! The console a program runs in: none when it isn't run from a terminal,
//! and the text typed ahead of a prompt.
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

/// Take the text waiting at the front of the console's input, up to the
/// first key that isn't text (Backspace, an arrow, a Ctrl combination).
///
/// Windows terminals type a pasted text in as keys, all waiting at once.
/// A prompt that takes them here can insert them in one edit instead of
/// redrawing itself after each key. Newlines read as `\n`. Other systems'
/// terminals hand a paste over whole, so nothing waits there.
#[allow(unsafe_code)]
pub fn take_typed_text() -> String {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            GetStdHandle, INPUT_RECORD, KEY_EVENT, PeekConsoleInputW, ReadConsoleInputW,
            STD_INPUT_HANDLE, WINDOW_BUFFER_SIZE_EVENT,
        };
        // SAFETY: no arguments to get wrong; without a console the calls
        // below refuse the handle.
        let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        let mut units = Vec::new();
        loop {
            // SAFETY: an all-zero record is a valid one to overwrite.
            let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
            let mut count = 0;
            // SAFETY: `record` is valid for the one record asked for.
            let peeked = unsafe { PeekConsoleInputW(input, &mut record, 1, &mut count) };
            if peeked == 0 || count == 0 {
                break;
            }
            let (step, repeat) = match u32::from(record.EventType) {
                KEY_EVENT => {
                    // SAFETY: the event type says which member is set.
                    let key = unsafe { record.Event.KeyEvent };
                    let step = typed::step(&typed::Key {
                        down: key.bKeyDown != 0,
                        code: key.wVirtualKeyCode,
                        // SAFETY: both members are plain integers.
                        unit: unsafe { key.uChar.UnicodeChar },
                        state: key.dwControlKeyState,
                    });
                    (step, key.wRepeatCount.max(1))
                }
                // The prompt lays itself out again on this one.
                WINDOW_BUFFER_SIZE_EVENT => (typed::Step::Stop, 0),
                _ => (typed::Step::Skip, 0),
            };
            if step == typed::Step::Stop {
                break;
            }
            // SAFETY: as for the peek; this takes the record peeked.
            unsafe { ReadConsoleInputW(input, &mut record, 1, &mut count) };
            if let typed::Step::Take(unit) = step {
                units.extend(std::iter::repeat_n(unit, usize::from(repeat)));
            }
        }
        String::from_utf16_lossy(&units)
    }
    #[cfg(not(windows))]
    String::new()
}

/// Which waiting console keys are text.
#[cfg(any(windows, test))]
mod typed {
    /// A key record: pressed or released, its virtual key code, its UTF-16
    /// unit (0 for keys that type nothing) and the modifier keys held.
    pub(super) struct Key {
        pub(super) down: bool,
        pub(super) code: u16,
        pub(super) unit: u16,
        pub(super) state: u32,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum Step {
        /// Text: take this UTF-16 unit.
        Take(u16),
        /// Nothing a prompt acts on: drop it and look at the next.
        Skip,
        /// A key the prompt must handle itself: leave it and what follows.
        Stop,
    }

    // `dwControlKeyState` bits and virtual key codes (Win32 console API).
    const RIGHT_ALT: u32 = 0x1;
    const LEFT_ALT: u32 = 0x2;
    const RIGHT_CTRL: u32 = 0x4;
    const LEFT_CTRL: u32 = 0x8;
    const VK_SHIFT: u16 = 0x10;
    const VK_CONTROL: u16 = 0x11;
    const VK_MENU: u16 = 0x12;
    const VK_CAPITAL: u16 = 0x14;
    /// Left and right Shift, Ctrl and Alt.
    const VK_SIDED: std::ops::RangeInclusive<u16> = 0xA0..=0xA5;

    pub(super) fn step(key: &Key) -> Step {
        if !key.down {
            // Releasing Alt types the character of an Alt+digits code.
            return if key.code == VK_MENU && key.unit != 0 {
                Step::Stop
            } else {
                Step::Skip
            };
        }
        let modifier = matches!(key.code, VK_SHIFT | VK_CONTROL | VK_MENU | VK_CAPITAL)
            || VK_SIDED.contains(&key.code);
        if modifier && key.unit == 0 {
            return Step::Skip;
        }
        // AltGr reads as left Ctrl with right Alt, and types text.
        let alt_gr = key.state & (LEFT_CTRL | RIGHT_ALT) == (LEFT_CTRL | RIGHT_ALT);
        if !alt_gr && key.state & (LEFT_CTRL | RIGHT_CTRL | LEFT_ALT | RIGHT_ALT) != 0 {
            return Step::Stop;
        }
        match key.unit {
            // Enter types a carriage return.
            0x0D | 0x0A => Step::Take(0x0A),
            0x09 => Step::Take(0x09),
            0..0x20 | 0x7F => Step::Stop,
            unit => Step::Take(unit),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn key(down: bool, code: u16, unit: u16, state: u32) -> Key {
            Key {
                down,
                code,
                unit,
                state,
            }
        }

        #[test]
        fn only_text_keys_are_taken() {
            let a = u16::from(b'a');
            assert_eq!(step(&key(true, 0x41, a, 0)), Step::Take(a));
            assert_eq!(step(&key(false, 0x41, a, 0)), Step::Skip);
            // Enter is a newline, Shift alone nothing.
            assert_eq!(step(&key(true, 0x0D, 0x0D, 0)), Step::Take(0x0A));
            assert_eq!(step(&key(true, VK_SHIFT, 0, 0)), Step::Skip);
            // Backspace, an arrow and Ctrl+A stay for the prompt.
            assert_eq!(step(&key(true, 0x08, 0x08, 0)), Step::Stop);
            assert_eq!(step(&key(true, 0x25, 0, 0)), Step::Stop);
            assert_eq!(step(&key(true, 0x41, 1, LEFT_CTRL)), Step::Stop);
            // AltGr types text.
            let at = u16::from(b'@');
            assert_eq!(
                step(&key(true, 0x51, at, LEFT_CTRL | RIGHT_ALT)),
                Step::Take(at)
            );
        }
    }
}
