//! The command bar: Luau typed at the terminal and run in a live
//! DataModel, like Roblox Studio's command bar, with the feel of Luau's own
//! REPL: a `> ` prompt with colored, multi-line editing ([`editor`]) and
//! history, kept between runs in the data folder (`state/command-bar-history.txt`).
//! Output prints above the prompt.
//!
//! Built only with the `repl` feature (on by default in the apps; build
//! with `--no-default-features` to leave it out). It reads this process's
//! own terminal and nothing else. Never connect it to the network
//! (`rook-net`) or a socket: anyone who could reach it could run any code
//! in the game.

use std::io::IsTerminal;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use reedline::Signal;
use rook_lua::MessageType;

use crate::DataModel;
use crate::console::{print_message, set_output};

mod editor;
mod highlight;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Server,
    Client,
}

const HELP: &str = "Type Luau to run it; an expression prints its value. \
                    :server and :client choose where commands run (server by default), \
                    :help shows this. Ctrl+J adds a line without running. \
                    Ctrl+C clears the line; Ctrl+D on an empty line ends input.";

#[derive(Debug)]
pub struct Repl {
    chunks: Receiver<String>,
    side: Side,
    /// Input ended (Ctrl+D on an empty line, or the terminal's input closed).
    closed: bool,
}

impl Repl {
    /// Start reading the terminal, if standard input is one.
    pub fn start() -> Option<Self> {
        if !std::io::stdin().is_terminal() {
            return None;
        }
        let (tx, rx) = channel();
        rook_platform::get()
            .threads
            .spawn("repl", Box::new(move || read(&tx)))
            .ok()?;
        print_message(MessageType::Info, HELP);
        Some(Self {
            chunks: rx,
            side: Side::Server,
            closed: false,
        })
    }

    pub fn side(&self) -> Side {
        self.side
    }

    pub fn closed(&self) -> bool {
        self.closed
    }

    /// Hand everything entered since the last call to `run`, with the side
    /// it should run on (see [`run_line`]); the caller may run the server
    /// on another thread. `has_client` is false in Run mode.
    pub fn poll(&mut self, has_client: bool, mut run: impl FnMut(Side, &str)) {
        loop {
            match self.chunks.try_recv() {
                Ok(chunk) => self.command(chunk.trim(), has_client, &mut run),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.closed = true;
                    break;
                }
            }
        }
    }

    fn command(&mut self, line: &str, has_client: bool, run: &mut impl FnMut(Side, &str)) {
        match line {
            "" => {}
            ":help" => print_message(MessageType::Info, HELP),
            ":server" => {
                self.side = Side::Server;
                print_message(MessageType::Info, "Commands run on the server");
            }
            ":client" if has_client => {
                self.side = Side::Client;
                print_message(MessageType::Info, "Commands run on the client");
            }
            ":client" => print_message(MessageType::Warning, "There is no client (Run mode)"),
            _ => run(self.side, line),
        }
    }
}

/// The reader thread: prompt, let the editor collect a complete chunk, and
/// send it to the engine.
fn read(tx: &Sender<String>) {
    // History from earlier runs, kept in the data folder's state.
    let history = crate::data_folder::DataDir::locate().map(|d| d.repl_history());
    let (mut editor, printer) = editor::new(history);
    set_output(Some(Box::new(move |line| {
        // Fails only once the prompt has closed, with nowhere to print.
        let _ = printer.send(line);
    })));
    let stopped = loop {
        match editor.read_line(&editor::LuauPrompt) {
            Ok(Signal::Success(chunk)) => {
                if tx.send(chunk).is_err() {
                    break None;
                }
            }
            Ok(Signal::CtrlD) => break None,
            // Ctrl+C clears the line without ending it (see `editor`).
            Ok(_) => {}
            Err(e) => break Some(e),
        }
    };
    // Output goes back to the terminal, restored, before anything more
    // prints.
    set_output(None);
    drop(editor);
    if let Some(e) = stopped {
        print_message(
            MessageType::Warning,
            &format!("The command bar stopped reading the terminal: {e}"),
        );
    }
}

/// Run one chunk as its own script thread, so it may yield. A chunk that
/// is an expression prints what it evaluates to; statements run as they
/// are. Globals persist from one chunk to the next. Compile errors come
/// back as `Err`; runtime errors appear in the output like any script's.
pub fn run_line(dm: &DataModel, line: &str) -> Result<(), String> {
    let lua = dm.runtime().lua();
    let is_expression = lua.load(format!("return {line}")).into_function().is_ok();
    let source = if is_expression {
        // On their own lines so a trailing `--` comment can't swallow the
        // closing parenthesis.
        format!(
            "local r = table.pack(\n{line}\n)\nif r.n > 0 then print(table.unpack(r, 1, r.n)) end"
        )
    } else {
        line.to_owned()
    };
    dm.runtime()
        .run_source_in("CommandBar", "CommandBar", &source)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Options;

    fn output(dm: &DataModel) -> Vec<String> {
        dm.take_log().into_iter().map(|e| e.message).collect()
    }

    #[test]
    fn expressions_print_and_statements_run() {
        let dm = DataModel::new(&Options::default()).unwrap();
        run_line(&dm, "1 + 1, 'two'").unwrap();
        run_line(&dm, "x = 5").unwrap();
        run_line(&dm, "x * 2 -- a comment").unwrap();
        // Calls that return nothing print nothing.
        run_line(&dm, "print('hi')").unwrap();
        assert_eq!(output(&dm), ["2 two", "10", "hi"]);
        assert!(run_line(&dm, "local = 1").is_err());
    }

    #[test]
    fn commands_may_yield() {
        let mut dm = DataModel::new(&Options::default()).unwrap();
        run_line(&dm, "task.wait(0.05) print('later')").unwrap();
        assert!(output(&dm).is_empty());
        dm.run_for(0.1);
        assert_eq!(output(&dm), ["later"]);
    }
}
