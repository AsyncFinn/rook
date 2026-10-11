//! `rook run-headless`: run a place's server Scripts without a window.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Context;
use rook_engine::{MessageType, Options, Server};

pub fn run(
    file: &Path,
    seconds: f64,
    timeout: f64,
    quiet: bool,
    script: Option<&Path>,
) -> anyhow::Result<()> {
    let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let options = Options {
        script_timeout: Duration::from_secs_f64(timeout),
        ..Options::default()
    };
    let started = Instant::now();
    let mut server = Server::load_place(&bytes, &options)?;
    server.start();
    if let Some(script) = script {
        let source = std::fs::read_to_string(script)
            .with_context(|| format!("reading {}", script.display()))?;
        server
            .runtime()
            .run_source("ServerScriptService.RookProbe", &source, None)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    server.run_for(seconds);
    let log = server.take_log();
    let errors = log.iter().filter(|e| e.kind == MessageType::Error).count();
    if !quiet {
        for entry in &log {
            rook_engine::console::print_entry(None, entry);
        }
    }
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
