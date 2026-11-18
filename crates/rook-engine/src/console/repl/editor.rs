//! The line editor: reedline with Luau colors, a multi-line buffer that
//! Enter runs only once the code is complete, and keys for choosing
//! yourself (Ctrl+J, or Shift, Ctrl or Alt+Enter, add a line).
//!
//! Pasting: terminals that support bracketed paste hand the whole text over
//! at once. Windows terminals type it in key by key, so a pasted newline
//! arrives as Enter and would run the code so far on its own, losing its
//! `local`s. [`PasteBurst`] tells reedline which keys were pasted, from how
//! fast they arrive, so the paste goes in as one edit with its newlines.

use std::borrow::Cow;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use mlua::chunk::Compiler;
use nu_ansi_term::Color;
use reedline::{
    EditCommand, Emacs, ExternalOutput, FileBackedHistory, Highlighter, KeyCode, KeyModifiers,
    PasteBurstHook, Prompt, PromptEditMode, PromptHistorySearch, Reedline, ReedlineEvent,
    StyledText, ValidationResult, Validator, default_emacs_keybindings,
};

use super::highlight;

/// Two keys arriving closer together than this were pasted. Typing a key
/// and then Enter takes people 50 ms at the very least; a pasted text's
/// keys arrive together.
const PASTE_GAP: Duration = Duration::from_millis(25);
/// Machine-fast keys in a row that make a paste, from then on gathered
/// into one edit.
const PASTE_KEYS: usize = 6;
/// History entries kept, oldest dropped first.
const HISTORY_SIZE: usize = 1000;

/// The editor, and where to send lines to print above its prompt.
pub(super) fn new(history: Option<PathBuf>) -> (Reedline, Sender<String>) {
    let mut keys = default_emacs_keybindings();
    let newline = ReedlineEvent::Edit(vec![EditCommand::InsertNewline]);
    // Ctrl+J types a line feed, which terminals send apart from Enter even
    // where they drop Shift from Shift+Enter (Windows Terminal), so it adds
    // a line everywhere. Shift and Alt+Enter already do.
    keys.add_binding(KeyModifiers::CONTROL, KeyCode::Char('j'), newline.clone());
    keys.add_binding(KeyModifiers::CONTROL, KeyCode::Enter, newline);
    // Ctrl+C is also what people press to copy a line of output, so it
    // clears what's typed and leaves the prompt open (Ctrl+D ends input).
    keys.add_binding(
        KeyModifiers::CONTROL,
        KeyCode::Char('c'),
        ReedlineEvent::Edit(vec![EditCommand::Clear]),
    );
    let (sender, output) = channel();
    let mut editor = Reedline::create()
        .with_edit_mode(Box::new(Emacs::new(keys)))
        .with_highlighter(Box::new(Luau))
        .with_validator(Box::new(Complete {
            compiler: Compiler::new(),
        }))
        .with_paste_burst(Arc::new(PasteBurst::default()))
        .use_bracketed_paste(true)
        .with_external_printer(Output(output));
    if let Some(path) = history {
        match FileBackedHistory::with_file(HISTORY_SIZE, path.clone()) {
            Ok(history) => editor = editor.with_history(Box::new(history)),
            Err(e) => tracing::debug!(target: "rook", "couldn't read {}: {e}", path.display()),
        }
    }
    (editor, sender)
}

/// The `> ` prompt, and `>> ` on each further line.
pub(super) struct LuauPrompt;

impl Prompt for LuauPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("> ")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed(">> ")
    }

    fn render_prompt_history_search_indicator(&self, _: PromptHistorySearch) -> Cow<'_, str> {
        Cow::Borrowed("? ")
    }

    // The prompt in the terminal's own text color, as Luau's REPL draws it.
    fn get_prompt_color(&self) -> Color {
        Color::Default
    }

    fn get_prompt_multiline_color(&self) -> Color {
        Color::Default
    }

    fn get_indicator_color(&self) -> Color {
        Color::Default
    }
}

struct Luau;

impl Highlighter for Luau {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        highlight::highlight(line)
    }
}

/// Unfinished code gets another line; anything else runs, compile errors
/// included, so they're reported as in Studio's command bar.
struct Complete {
    /// Compiles without a VM, so the editor's thread needs no Lua state.
    compiler: Compiler,
}

impl Validator for Complete {
    fn validate(&self, line: &str) -> ValidationResult {
        if is_unfinished(&self.compiler, line) {
            ValidationResult::Incomplete
        } else {
            ValidationResult::Complete
        }
    }
}

/// Whether `source` only fails to compile because it ends too soon (an
/// open `do`, `function`, table or string), so Enter should add a line,
/// as in Luau's REPL.
fn is_unfinished(compiler: &Compiler, source: &str) -> bool {
    if source.trim_start().starts_with(':') {
        return false;
    }
    // `Err(true)` when it ends too soon.
    let compiles = |s: &str| {
        compiler.compile(s).map(drop).map_err(|e| {
            matches!(
                e,
                mlua::Error::SyntaxError {
                    incomplete_input: true,
                    ..
                }
            )
        })
    };
    // An expression compiles as `return` and one.
    match (compiles(source), compiles(&format!("return {source}"))) {
        (Ok(()), _) | (_, Ok(())) => false,
        (Err(ends_too_soon), _) => ends_too_soon,
    }
}

/// Lines to print above the prompt. Unbounded, so the engine never waits
/// on the terminal to print.
struct Output(Receiver<String>);

impl ExternalOutput for Output {
    fn drain(&mut self) -> io::Result<Vec<String>> {
        Ok(self.0.try_iter().collect())
    }
}

/// Tells a paste typed in as keys from typing, by how fast the keys come.
#[derive(Default)]
struct PasteBurst(Mutex<Burst>);

#[derive(Default)]
struct Burst {
    /// When the last of a run of quick keys arrived.
    last: Option<Instant>,
    /// How many keys that run holds.
    keys: usize,
    /// A paste is being gathered, until reedline settles it.
    pasting: bool,
}

impl Burst {
    fn quick(&self, now: Instant) -> bool {
        self.last
            .is_some_and(|last| now.duration_since(last) < PASTE_GAP)
    }

    fn key(&mut self, now: Instant) {
        self.keys = if self.quick(now) { self.keys + 1 } else { 1 };
        self.last = Some(now);
        self.pasting |= self.keys >= PASTE_KEYS;
    }

    /// Whether an Enter at `now` was pasted. A short paste never counts
    /// as one, but its Enters still come right behind its keys.
    fn enter_is_newline(&mut self, now: Instant) -> bool {
        let pasted = self.pasting || self.quick(now);
        if pasted {
            // The next pasted line continues the run.
            self.last = Some(now);
        }
        pasted
    }
}

impl PasteBurst {
    fn burst(&self) -> std::sync::MutexGuard<'_, Burst> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl PasteBurstHook for PasteBurst {
    fn on_char(&self, _: char) {
        self.burst().key(Instant::now());
    }

    fn enter_is_newline(&self) -> bool {
        self.burst().enter_is_newline(Instant::now())
    }

    fn is_burst_active(&self) -> bool {
        self.burst().pasting
    }

    fn poll_timeout(&self) -> Duration {
        PASTE_GAP
    }

    fn settle(&self) {
        *self.burst() = Burst::default();
    }

    fn resolve_burst(&self, _: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfinished_statements_ask_for_more() {
        let compiler = Compiler::new();
        for open in [
            "for i = 1, 3 do",
            "local t = {",
            "function f()",
            "if x then print(1)",
            "x = 1 +",
            "print(",
        ] {
            assert!(is_unfinished(&compiler, open), "{open}");
        }
        for done in [
            "for i = 1, 3 do end",
            "1 + 1",
            "local x = 5",
            ":client",
            "local = 1",
            "end",
        ] {
            assert!(!is_unfinished(&compiler, done), "{done}");
        }
    }

    #[test]
    fn only_quick_keys_make_a_paste() {
        let start = Instant::now();
        let ms = |n| start + Duration::from_millis(n);
        let mut burst = Burst::default();
        for n in 0..PASTE_KEYS as u64 - 1 {
            burst.key(ms(n));
        }
        assert!(!burst.pasting);
        // A short paste's Enter, right behind its keys, adds a line.
        assert!(burst.enter_is_newline(ms(5)));
        burst.key(ms(6));
        assert!(burst.pasting);

        let mut typed = Burst::default();
        for n in 0..10 {
            typed.key(ms(n * 100));
        }
        assert!(!typed.pasting);
        assert!(!typed.enter_is_newline(ms(1000)));
    }
}
