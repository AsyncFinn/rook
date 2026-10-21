//! The line editor: rustyline with Luau colors, a multi-line buffer that
//! Enter runs only once the code is complete, and keys for choosing
//! yourself (Shift, Ctrl or Alt+Enter add a line).
//!
//! Pasting: terminals that support bracketed paste (most on Linux and
//! macOS) hand rustyline the whole text at once. Windows terminals type it
//! in key by key, so each pasted newline would arrive as Enter and run the
//! code so far on its own, losing its `local`s. An Enter that arrives right
//! after the key before it was pasted, not typed, and adds a line instead.

use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustyline::completion::Completer;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::history::FileHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{
    Cmd, ConditionalEventHandler, Editor, Event, EventContext, EventHandler, Helper, KeyCode,
    KeyEvent, Modifiers, Movement, RepeatCount,
};

use super::{highlight, is_unfinished};

/// How soon after the previous key an Enter counts as pasted. Typing a key
/// and then Enter takes people 50 ms at the very least; a pasted text's
/// keys arrive as fast as the editor redraws.
const PASTE_GAP: Duration = Duration::from_millis(25);

pub(super) fn new() -> rustyline::Result<Editor<Luau, FileHistory>> {
    let mut editor = Editor::new()?;
    editor.set_helper(Some(Luau {
        checker: mlua::Lua::new(),
    }));
    for modifier in [Modifiers::SHIFT, Modifiers::CTRL, Modifiers::ALT] {
        editor.bind_sequence(KeyEvent(KeyCode::Enter, modifier), Cmd::Newline);
    }
    let keys = Arc::new(KeyClock::default());
    // Ctrl+J and Ctrl+M are Enter too, and some terminals type a pasted
    // newline as one of them.
    for enter in [
        KeyEvent(KeyCode::Enter, Modifiers::NONE),
        KeyEvent::ctrl('J'),
        KeyEvent::ctrl('M'),
    ] {
        editor.bind_sequence(
            enter,
            EventHandler::Conditional(Box::new(Enter(keys.clone()))),
        );
    }
    editor.bind_sequence(
        Event::Any,
        EventHandler::Conditional(Box::new(AnyKey(keys))),
    );
    editor.bind_sequence(
        KeyEvent::ctrl('C'),
        EventHandler::Conditional(Box::new(Interrupt)),
    );
    Ok(editor)
}

#[derive(Debug)]
pub(super) struct Luau {
    /// A VM of the editor's own, only for checking whether input is complete.
    checker: mlua::Lua,
}

impl Helper for Luau {}

impl Completer for Luau {
    type Candidate = String;
}

impl Hinter for Luau {
    type Hint = String;
}

impl Highlighter for Luau {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Owned(highlight::highlight(line))
    }

    fn highlight_char(&self, _line: &str, _pos: usize, kind: CmdKind) -> bool {
        kind != CmdKind::MoveCursor
    }
}

impl Validator for Luau {
    /// Unfinished code gets another line; anything else runs, compile
    /// errors included, so they're reported as in Studio's command bar.
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        Ok(if is_unfinished(&self.checker, ctx.input()) {
            ValidationResult::Incomplete
        } else {
            ValidationResult::Valid(None)
        })
    }
}

/// When the last key arrived.
#[derive(Debug, Default)]
struct KeyClock(Mutex<Option<Instant>>);

impl KeyClock {
    /// Note a key at `now`, and whether it followed the previous one within
    /// [`PASTE_GAP`].
    fn press(&self, now: Instant) -> bool {
        let mut last = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let pasted = last.is_some_and(|last| now.duration_since(last) < PASTE_GAP);
        *last = Some(now);
        pasted
    }
}

#[derive(Debug)]
struct AnyKey(Arc<KeyClock>);

impl ConditionalEventHandler for AnyKey {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, _: &EventContext) -> Option<Cmd> {
        self.0.press(Instant::now());
        None
    }
}

#[derive(Debug)]
struct Enter(Arc<KeyClock>);

impl ConditionalEventHandler for Enter {
    /// A pasted Enter adds a line; a typed one does what Enter does.
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, _: &EventContext) -> Option<Cmd> {
        self.0.press(Instant::now()).then_some(Cmd::Newline)
    }
}

#[derive(Debug)]
struct Interrupt;

impl ConditionalEventHandler for Interrupt {
    /// Ctrl+C clears what's typed, all its lines; on an empty prompt it
    /// ends input as usual.
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        (!ctx.line().is_empty()).then_some(Cmd::Kill(Movement::WholeBuffer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_quick_keys_count_as_pasted() {
        let clock = KeyClock::default();
        let start = Instant::now();
        assert!(!clock.press(start));
        assert!(clock.press(start + Duration::from_millis(1)));
        assert!(!clock.press(start + Duration::from_millis(200)));
    }
}
