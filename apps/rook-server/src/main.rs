//! `rook-server`: run a place's server by itself at 60 Hz of real time, as
//! a dedicated server does, with no window, renderer or audio device
//! (docs/design/architecture.md#crate-layout). Its scripts run and print
//! to the terminal, and with the `repl` feature the terminal's `> ` prompt
//! runs Luau on the server.
//!
//! Players can't join it yet: replication runs within one process until
//! `rook-net` has a network transport (docs/roadmap/README.md).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use rook_content::Resolver;
use rook_engine::session::server_thread::ServerThread;
use rook_engine::{Options, settings};

#[derive(Debug, Parser)]
#[command(
    name = "rook-server",
    version,
    about = "Run a Roblox place's server in Rook"
)]
struct Args {
    /// Place file (.rbxl, .rbxlx, .rbxm or .rbxmx).
    place: PathBuf,
    /// Also run this Luau file as a server script.
    #[arg(long)]
    script: Option<PathBuf>,
    /// Stop after this many seconds (default: run until stopped).
    #[arg(long)]
    seconds: Option<f64>,
    /// Don't show the `>` command prompt in the terminal.
    #[cfg(feature = "repl")]
    #[arg(long)]
    no_repl: bool,
    /// `--verbose <subsystem>` (content, terrain, ...) shows everything
    /// that area logs.
    #[arg(long, value_name = "SUBSYSTEM", num_args = 0..=1, require_equals = true, default_missing_value = "")]
    verbose: Option<String>,
}

/// How often the server's finished frames are let go of: no client takes
/// them yet.
const DRAIN: Duration = Duration::from_millis(50);

fn main() -> anyhow::Result<()> {
    let argv = rook_engine::logging::fix_verbose_args(std::env::args());
    let args = Args::parse_from(argv);
    rook_engine::logging::init(&rook_engine::logging::Options {
        program: "rook-server",
        verbose: args.verbose.clone(),
        file: true,
    });
    rook_engine::logging::run(|| serve(&args), |_| {})
}

fn serve(args: &Args) -> anyhow::Result<()> {
    let place = fs_err::read(&args.place)?;
    let script = match &args.script {
        Some(path) => Some(fs_err::read_to_string(path)?),
        None => None,
    };
    // A server loads what its place uses (meshes' collision shapes), as
    // the player does: asset delivery is on unless the setting says not.
    let options = Options {
        content: Some(Arc::new(Resolver::new(settings::content_config(true)))),
        ..Options::default()
    };
    let (server, _) = ServerThread::spawn(place, options, script, None)?.wait()?;
    server.run(true);
    #[cfg(feature = "repl")]
    let mut repl = if args.no_repl {
        None
    } else {
        rook_engine::console::repl::Repl::start()
    };
    let started = Instant::now();
    while args
        .seconds
        .is_none_or(|s| started.elapsed().as_secs_f64() < s)
    {
        std::thread::sleep(DRAIN);
        server.frames().for_each(drop);
        #[cfg(feature = "repl")]
        if let Some(repl) = &mut repl {
            repl.poll(false, |_, line| server.repl(line));
            if repl.closed() {
                break;
            }
        }
    }
    Ok(())
}
