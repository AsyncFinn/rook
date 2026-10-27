//! The thread that writes printed lines to the terminal and the log file.
//!
//! A terminal write is slow on Windows consoles and the log file flushes
//! every line, so a game that prints or errors every frame would pay both
//! in frame time if the printing thread wrote them itself. Lines go
//! through a channel instead, in the order they were printed.

use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Mutex, OnceLock};

enum Job {
    /// A line with its color codes, for the log file and, if `terminal`,
    /// the terminal.
    Line { text: String, terminal: bool },
    /// Answer once every line sent before it is written.
    Flush(SyncSender<()>),
}

static JOBS: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();

fn jobs() -> &'static Mutex<Sender<Job>> {
    JOBS.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        rook_platform::get()
            .threads
            .spawn("rook-output", Box::new(move || write(&rx)))
            .expect("start the output thread");
        Mutex::new(tx)
    })
}

fn send(job: Job) -> bool {
    jobs()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .send(job)
        .is_ok()
}

/// Queue a line for the log file and, if `terminal`, the terminal.
pub(super) fn line(text: String, terminal: bool) {
    send(Job::Line { text, terminal });
}

/// Wait until every line printed so far is written. Returns at once if
/// the output thread has died (a panicking prompt), since nothing is left
/// to wait for.
pub fn flush() {
    let (tx, rx) = mpsc::sync_channel(1);
    if send(Job::Flush(tx)) {
        let _ = rx.recv();
    }
}

fn write(rx: &Receiver<Job>) {
    // Read once: an environment lookup per line costs more than the line.
    let no_color = std::env::var_os("NO_COLOR").is_some();
    for job in rx {
        match job {
            Job::Line { text, terminal } => {
                crate::logging::record(&text);
                if terminal {
                    super::show(text, no_color);
                }
            }
            Job::Flush(done) => {
                let _ = done.send(());
            }
        }
    }
}
