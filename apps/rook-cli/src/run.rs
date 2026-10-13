//! `rook run-headless`: run a place's server Scripts without a window.

use std::path::Path;
use std::time::{Duration, Instant};

use rook_engine::{MessageType, Options, Server};

pub fn run(
    file: &Path,
    seconds: f64,
    timeout: f64,
    quiet: bool,
    script: Option<&Path>,
    #[cfg(feature = "repl")] repl: bool,
) -> anyhow::Result<()> {
    let bytes = fs_err::read(file)?;
    let options = Options {
        script_timeout: Duration::from_secs_f64(timeout),
        ..Options::default()
    };
    let started = Instant::now();
    let mut server = Server::load_place(&bytes, &options)?;
    server.start();
    if let Some(script) = script {
        let source = fs_err::read_to_string(script)?;
        server
            .run_extra_script(&source)
            .map_err(anyhow::Error::msg)?;
    }
    server.run_for(seconds);
    let log = server.take_log();
    if !quiet {
        for entry in &log {
            rook_engine::console::print_entry("Server", entry);
        }
    }
    #[cfg(feature = "repl")]
    let log = if repl {
        [log, interactive(&mut server)].concat()
    } else {
        log
    };
    let errors = log.iter().filter(|e| e.kind == MessageType::Error).count();
    let world = server.runtime().world();
    println!(
        "\n{} scripts started, {} log lines, {errors} errors, {:.1}s simulated in {:.2}s",
        server.started_scripts(),
        log.len(),
        server.now(),
        started.elapsed().as_secs_f64()
    );
    if !world.unimplemented.is_empty() {
        println!("Not implemented in Rook yet (calls):");
        let mut list: Vec<_> = world.unimplemented.iter().collect();
        list.sort_by(|a, b| b.1.cmp(a.1));
        for (member, n) in list.iter().take(20) {
            println!("  {n:>6}  {member}");
        }
    }
    if !world.inert.is_empty() {
        println!("Accepted but ignored for now (calls):");
        for (member, n) in &world.inert {
            println!("  {n:>6}  {member}");
        }
    }
    Ok(())
}

/// Run in real time at 60 Hz with the command prompt until input ends,
/// printing output as it comes. Returns the output.
#[cfg(feature = "repl")]
fn interactive(server: &mut Server) -> Vec<rook_lua::LogEntry> {
    let Some(mut repl) = rook_engine::repl::Repl::start() else {
        rook_engine::console::print_message(
            MessageType::Warning,
            "--repl needs an interactive terminal",
        );
        return Vec::new();
    };
    let frame = Duration::from_secs_f64(1.0 / 60.0);
    let mut next = Instant::now();
    let mut log = Vec::new();
    while !repl.closed() {
        repl.poll(false, |_, line| {
            if let Err(e) = rook_engine::repl::run_line(server, line) {
                rook_engine::console::print_message(MessageType::Error, &e);
            }
        });
        server.step();
        for entry in server.take_log() {
            rook_engine::console::print_entry("Server", &entry);
            log.push(entry);
        }
        next += frame;
        match next.checked_duration_since(Instant::now()) {
            Some(wait) => std::thread::sleep(wait),
            None => next = Instant::now(),
        }
    }
    log
}
