//! The server on its own thread, so a window waits only on its client and
//! the renderer (architecture §8). The server's DataModel is made on the
//! thread and never leaves it; the window talks to it in messages.
//!
//! The thread steps when told to ([`ServerThread::step`], which keeps a
//! client in lockstep with it, so `--seconds`, `--bench` and screenshots
//! give the same results every run), or by itself at 60 Hz of real time
//! ([`ServerThread::run`]), as a server does for players.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rook_engine::session::host::{Joined, ServerHost, ToClient, ToServer};
use rook_engine::{DataModel, MessageType, Options, console};
use rook_render::Camera;

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
    Repl(String),
}

/// What the server sends back once it's running.
#[derive(Debug)]
pub struct Started {
    pub joined: Joined,
    /// The camera the place was saved with.
    pub camera: Camera,
}

#[derive(Debug)]
pub struct ServerThread {
    commands: Sender<Command>,
    frames: Receiver<Frame>,
    thread: Option<JoinHandle<()>>,
}

/// How far behind real time a running server may fall before it stops
/// catching up and drops the lost time (as the window's client does).
const MAX_LAG: Duration = Duration::from_millis(250);

/// A server thread still loading its place: [`Starting::poll`] it each
/// frame, or [`Starting::wait`].
pub struct Starting {
    server: Option<ServerThread>,
    started: Receiver<anyhow::Result<Started>>,
}

impl Starting {
    /// The server, once it has loaded and its player has joined.
    pub fn poll(&mut self) -> Option<anyhow::Result<(ServerThread, Started)>> {
        let started = match self.started.try_recv() {
            Ok(started) => started,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(anyhow::anyhow!("the server thread stopped")),
        };
        Some(started.map(|s| (self.server.take().expect("polled after it started"), s)))
    }

    pub fn wait(mut self) -> anyhow::Result<(ServerThread, Started)> {
        let started = self
            .started
            .recv()
            .map_err(|_| anyhow::anyhow!("the server thread stopped"))??;
        Ok((self.server.take().expect("waited once"), started))
    }
}

impl ServerThread {
    /// Load `place` on a new thread, start its scripts, run `script` (the
    /// `--script` file) and let one player join.
    pub fn spawn(
        place: Vec<u8>,
        options: Options,
        script: Option<String>,
        player: (&str, i64),
    ) -> anyhow::Result<Starting> {
        let (commands, command_rx) = channel();
        let (frame_tx, frames) = channel();
        let (started_tx, started_rx) = channel();
        let (name, user_id) = (player.0.to_owned(), player.1);
        let thread = std::thread::Builder::new()
            .name("server".into())
            .spawn(move || {
                let host = (|| -> anyhow::Result<ServerHost> {
                    let mut server = DataModel::load_place(&place, &options)?;
                    server.start();
                    let mut host = ServerHost::new(server);
                    let camera = crate::scene::initial_camera(host.server());
                    let (_, joined) = host.join(&name, user_id)?;
                    if let Some(source) = &script {
                        host.server()
                            .run_extra_script(source)
                            .map_err(anyhow::Error::msg)?;
                    }
                    print_logs(host.server());
                    let _ = started_tx.send(Ok(Started { joined, camera }));
                    Ok(host)
                })();
                match host {
                    Ok(host) => serve(host, &command_rx, &frame_tx),
                    Err(e) => {
                        let _ = started_tx.send(Err(e));
                    }
                }
            })?;
        Ok(Starting {
            server: Some(Self {
                commands,
                frames,
                thread: Some(thread),
            }),
            started: started_rx,
        })
    }

    /// Step the server once and wait for its frame.
    pub fn step(&self) -> Frame {
        let _ = self.commands.send(Command::Step);
        self.frames.recv().unwrap_or(Frame {
            messages: Vec::new(),
            took: Duration::ZERO,
        })
    }

    /// Let the server step by itself at 60 Hz (`true`), or only when told.
    pub fn run(&self, on: bool) {
        let _ = self.commands.send(Command::Run(on));
    }

    /// Frames the running server has finished since the last call.
    pub fn frames(&self) -> impl Iterator<Item = Frame> + '_ {
        self.frames.try_iter()
    }

    pub fn deliver(&self, msg: ToServer) {
        let _ = self.commands.send(Command::Deliver(msg));
    }

    /// Run a command-bar entry on the server.
    pub fn repl(&self, line: &str) {
        let _ = self.commands.send(Command::Repl(line.to_owned()));
    }
}

impl Drop for ServerThread {
    fn drop(&mut self) {
        // Closing the command channel ends the thread.
        let (dead, _) = channel();
        self.commands = dead;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn print_logs(server: &DataModel) {
    for entry in server.take_log() {
        console::print_entry("Server", &entry);
    }
}

/// The server thread's loop, until the window goes away.
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

/// Carry out one command. Returns false once the window is gone.
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
        Command::Repl(line) => {
            if let Err(e) = rook_engine::console::repl::run_line(host.server(), &line) {
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
    let updates = host.updates().pop().unwrap_or_default();
    messages.push(ToClient::Updates(updates));
    print_logs(host.server());
    Frame {
        messages,
        took: start.elapsed(),
    }
}
