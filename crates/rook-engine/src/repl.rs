//! The command bar: Luau typed at the terminal and run in a live
//! DataModel, like Roblox Studio's command bar, with the feel of Luau's own
//! REPL: `> ` prompts, `>> ` while a statement is unfinished, line editing
//! and history (rustyline). Output prints above the prompt.
//!
//! Built only with the `repl` feature (on by default in the apps; build
//! with `--no-default-features` to leave it out). It reads this process's
//! own terminal and nothing else. Never connect it to the network
//! (`rook-net`) or a socket: anyone who could reach it could run any code
//! in the game.

use std::io::IsTerminal;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use rook_lua::MessageType;
use rustyline::error::ReadlineError;
use rustyline::{DefaultEditor, ExternalPrinter};

use crate::DataModel;
use crate::console::{print_message, set_output};

/// Where commands run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Server,
    Client,
}

const HELP: &str = "Type Luau to run it; an expression prints its value. \
                    :server and :client choose where commands run (server by default), \
                    :help shows this. Ctrl+C clears the line; on an empty line it, or Ctrl+D, ends input.";

#[derive(Debug)]
pub struct Repl {
    chunks: Receiver<String>,
    side: Side,
    /// Input ended (Ctrl+C on an empty line, or Ctrl+D).
    closed: bool,
}

impl Repl {
    /// Start reading the terminal, if standard input is one.
    pub fn start() -> Option<Self> {
        if !std::io::stdin().is_terminal() {
            return None;
        }
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("repl".into())
            .spawn(move || read(&tx))
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

    /// Whether input has ended.
    pub fn closed(&self) -> bool {
        self.closed
    }

    /// Run everything entered since the last call, on the server or the
    /// client (`None` when there is no client).
    pub fn poll(&mut self, server: &DataModel, client: Option<&DataModel>) {
        loop {
            match self.chunks.try_recv() {
                Ok(chunk) => self.command(chunk.trim(), server, client),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.closed = true;
                    break;
                }
            }
        }
    }

    fn command(&mut self, line: &str, server: &DataModel, client: Option<&DataModel>) {
        match line {
            "" => {}
            ":help" => print_message(MessageType::Info, HELP),
            ":server" => {
                self.side = Side::Server;
                print_message(MessageType::Info, "Commands run on the server");
            }
            ":client" => match client {
                Some(_) => {
                    self.side = Side::Client;
                    print_message(MessageType::Info, "Commands run on the client");
                }
                None => print_message(MessageType::Warning, "There is no client (Run mode)"),
            },
            _ => {
                let dm = match (self.side, client) {
                    (Side::Client, Some(c)) => c,
                    _ => server,
                };
                if let Err(e) = run_line(dm, line) {
                    print_message(MessageType::Error, &e);
                }
            }
        }
    }
}

/// The reader thread: prompt, collect a complete chunk (asking for more
/// lines while a statement is unfinished), and send it to the engine.
fn read(tx: &Sender<String>) {
    let Ok(mut editor) = DefaultEditor::new() else {
        return;
    };
    if let Ok(mut printer) = editor.create_external_printer() {
        set_output(Some(Box::new(move |line| {
            let _ = printer.print(line);
        })));
    }
    // A VM of the reader's own, only for checking whether input is complete.
    let checker = mlua::Lua::new();
    let mut pending = String::new();
    loop {
        let prompt = if pending.is_empty() { "> " } else { ">> " };
        match editor.readline(prompt) {
            Ok(line) => {
                if !pending.is_empty() {
                    pending.push('\n');
                }
                pending.push_str(&line);
                if is_unfinished(&checker, &pending) {
                    continue;
                }
                let chunk = std::mem::take(&mut pending);
                if !chunk.trim().is_empty() {
                    let _ = editor.add_history_entry(chunk.as_str());
                }
                if tx.send(chunk).is_err() {
                    break;
                }
            }
            Err(ReadlineError::Interrupted) if !pending.is_empty() => pending.clear(),
            Err(_) => break,
        }
    }
    set_output(None);
}

/// Whether `source` only fails to compile because it ends too soon (an
/// open `do`, `function`, table or string), so the prompt should ask for
/// more, as Luau's REPL does.
fn is_unfinished(lua: &mlua::Lua, source: &str) -> bool {
    if source.trim_start().starts_with(':') {
        return false;
    }
    let compiles = |s: &str| lua.load(s).into_function().map_err(|e| e.to_string());
    match (compiles(source), compiles(&format!("return {source}"))) {
        (Ok(_), _) | (_, Ok(_)) => false,
        (Err(e), _) => e.contains("got <eof>"),
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
    fn unfinished_statements_ask_for_more() {
        let lua = mlua::Lua::new();
        for open in [
            "for i = 1, 3 do",
            "local t = {",
            "function f()",
            "if x then print(1)",
            "x = 1 +",
        ] {
            assert!(is_unfinished(&lua, open), "{open}");
        }
        for done in [
            "for i = 1, 3 do end",
            "1 + 1",
            "print(",
            ":client",
            "local = 1",
            "end",
        ] {
            let unfinished = is_unfinished(&lua, done);
            assert_eq!(unfinished, done == "print(", "{done}");
        }
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
