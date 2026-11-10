//! The server on its own thread, so a window waits only on its client and
//! the renderer, and a dedicated server keeps its 60 Hz pace
//! (docs/design/architecture.md#engine-systems). The server's DataModel is
//! made on the thread and never leaves it; the rest of the app talks to it
//! in messages.
//!
//! The thread steps when told to ([`ServerThread::step`], which keeps a
//! client in lockstep with it, so `--seconds`, `--bench` and screenshots
//! give the same results every run), or by itself at 60 Hz of real time
//! ([`ServerThread::run`]), as a server does for players.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::time::{Duration, Instant};

#[cfg(feature = "repl")]
use rook_lua::MessageType;

use crate::session::host::{Joined, ServerHost, ToClient, ToServer};
use crate::{DataModel, EngineError, Options, SavedCamera, console};

/// One server frame's output for the client.
#[derive(Debug)]
pub struct Frame {
    pub messages: Vec<ToClient>,
    /// How long the server took to step.
    pub took: Duration,
}

#[derive(Debug)]
enum Command {
    Step,
    /// Step at 60 Hz by itself, or stop doing so.
    Run(bool),
    Deliver(ToServer),
    #[cfg(feature = "repl")]
    Repl(String),
}

/// What the server sends back once it's running.
#[derive(Debug)]
pub struct Started {
    /// The player who joined, if one was asked for.
    pub joined: Option<Joined>,
    /// The camera the place was saved with.
    pub camera: Option<SavedCamera>,
}

#[derive(Debug)]
pub struct ServerThread {
    commands: Sender<Command>,
    frames: Receiver<Frame>,
    /// Closed once the thread has ended, its DataModel dropped.
    ended: Receiver<()>,
}

/// How far behind real time a running server may fall before it stops
/// catching up and drops the lost time (as the window's client does).
const MAX_LAG: Duration = Duration::from_millis(250);

/// A server thread still loading its place: [`Starting::poll`] it each
/// frame, or [`Starting::wait`].
#[derive(Debug)]
pub struct Starting {
    server: Option<ServerThread>,
    started: Receiver<Result<Started, EngineError>>,
}

impl Starting {
    /// The server, once it has loaded and its player has joined.
    pub fn poll(&mut self) -> Option<Result<(ServerThread, Started), EngineError>> {
        let started = match self.started.try_recv() {
            Ok(started) => started,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(EngineError::ServerStopped),
        };
        Some(started.map(|s| (self.server.take().expect("polled after it started"), s)))
    }

    pub fn wait(mut self) -> Result<(ServerThread, Started), EngineError> {
        let started = self
            .started
            .recv()
            .map_err(|_| EngineError::ServerStopped)??;
        Ok((self.server.take().expect("waited once"), started))
    }
}

impl ServerThread {
    /// Load `place` on a new thread, start its scripts, run `script` (the
    /// `--script` file) and let `player` (name, user id) join, if given.
    pub fn spawn(
        place: Vec<u8>,
        options: Options,
        script: Option<String>,
        player: Option<(&str, i64)>,
    ) -> Result<Starting, EngineError> {
        let (commands, command_rx) = channel();
        let (frame_tx, frames) = channel();
        let (started_tx, started_rx) = channel();
        let (ended_tx, ended) = channel::<()>();
        let player = player.map(|(name, user_id)| (name.to_owned(), user_id));
        let body = move || {
            // Dropped as the thread ends, however it ends.
            let _ended = ended_tx;
            let host = (|| -> Result<ServerHost, EngineError> {
                let mut server = DataModel::load_place(&place, &options)?;
                server.start();
                let mut host = ServerHost::new(server);
                let camera = SavedCamera::read(host.server());
                let joined = match &player {
                    Some((name, user_id)) => Some(host.join(name, *user_id)?.1),
                    None => None,
                };
                if let Some(source) = &script {
                    host.server()
                        .run_extra_script(source)
                        .map_err(EngineError::Script)?;
                }
                print_logs(host.server());
                // The receiver is gone only if the game closed while
                // starting.
                let _ = started_tx.send(Ok(Started { joined, camera }));
                Ok(host)
            })();
            match host {
                Ok(host) => serve(host, &command_rx, &frame_tx),
                Err(e) => {
                    let _ = started_tx.send(Err(e));
                }
            }
        };
        rook_platform::get()
            .threads
            .spawn("server", Box::new(body))?;
        Ok(Starting {
            server: Some(Self {
                commands,
                frames,
                ended,
            }),
            started: started_rx,
        })
    }

    /// Send the server thread a command. It is gone only if it failed,
    /// which the game has reported, or the game is closing.
    fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Step the server once and wait for its frame.
    pub fn step(&self) -> Frame {
        self.send(Command::Step);
        self.frames.recv().unwrap_or(Frame {
            messages: Vec::new(),
            took: Duration::ZERO,
        })
    }

    /// Let the server step by itself at 60 Hz (`true`), or only when told.
    pub fn run(&self, on: bool) {
        self.send(Command::Run(on));
    }

    /// Frames the running server has finished since the last call.
    pub fn frames(&self) -> impl Iterator<Item = Frame> + '_ {
        self.frames.try_iter()
    }

    pub fn deliver(&self, msg: ToServer) {
        self.send(Command::Deliver(msg));
    }

    /// Run a command-bar entry on the server.
    #[cfg(feature = "repl")]
    pub fn repl(&self, line: &str) {
        self.send(Command::Repl(line.to_owned()));
    }
}

impl Drop for ServerThread {
    fn drop(&mut self) {
        // Closing the command channel ends the thread; wait until it has,
        // so its DataModel is gone (and its last output printed) when this
        // returns. A panic was reported where it happened.
        let (dead, _) = channel();
        self.commands = dead;
        let _ = self.ended.recv();
    }
}

fn print_logs(server: &DataModel) {
    for entry in server.take_log() {
        console::print_entry("Server", &entry);
    }
}

/// The server thread's loop, until the app lets it go.
fn serve(mut host: ServerHost, commands: &Receiver<Command>, frames: &Sender<Frame>) {
    let step = Duration::from_secs_f64(1.0 / 60.0);
    let mut running = false;
    let mut next = Instant::now();
    loop {
        // Everything the client sent goes in before the next step, even
        // when the server is behind and stepping back to back.
        loop {
            match commands.try_recv() {
                Ok(command) => {
                    if !obey(&mut host, command, frames, &mut running, &mut next) {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let command = if running {
            let now = Instant::now();
            if now >= next {
                if frames.send(frame(&mut host)).is_err() {
                    return;
                }
                next += step;
                if now.duration_since(next) > MAX_LAG {
                    next = now;
                }
                continue;
            }
            match commands.recv_timeout(next - now) {
                Ok(c) => c,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match commands.recv() {
                Ok(c) => c,
                Err(_) => return,
            }
        };
        if !obey(&mut host, command, frames, &mut running, &mut next) {
            return;
        }
    }
}

/// Carry out one command. Returns false once the app is gone.
fn obey(
    host: &mut ServerHost,
    command: Command,
    frames: &Sender<Frame>,
    running: &mut bool,
    next: &mut Instant,
) -> bool {
    match command {
        Command::Step => return frames.send(frame(host)).is_ok(),
        Command::Run(on) => {
            *running = on;
            *next = Instant::now();
        }
        Command::Deliver(msg) => host.deliver(0, msg),
        #[cfg(feature = "repl")]
        Command::Repl(line) => {
            if let Err(e) = crate::console::repl::run_line(host.server(), &line) {
                console::print_message(MessageType::Error, &e);
            }
            print_logs(host.server());
        }
    }
    true
}

/// One server frame, with what the client should receive.
fn frame(host: &mut ServerHost) -> Frame {
    let start = Instant::now();
    host.step();
    let mut messages: Vec<ToClient> = host.outgoing().into_iter().map(|(_, m)| m).collect();
    messages.extend(host.updates().pop());
    print_logs(host.server());
    Frame {
        messages,
        took: start.elapsed(),
    }
}
