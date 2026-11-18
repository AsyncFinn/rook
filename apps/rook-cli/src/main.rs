//! `rook`: open a place, and look after Rook's own files (its cache, logs
//! and Open Cloud key).
//!
//! `rook place.rbxl` opens it in the player, as dragging it onto `rook.exe`
//! does. Opened from Explorer with nothing to open, it asks for a place;
//! run bare in a terminal, it prints its help. Tools for looking inside
//! places and working on Rook are `cargo xtask` (apps/xtask).

mod commands;

use commands::{cache, key, logs, play};
use std::ffi::OsString;

use clap::{CommandFactory, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "rook", version, about = "Open Roblox places with Rook")]
struct Cli {
    /// None only from a terminal (a bare `rook` elsewhere asks for a
    /// place first), where it prints the help.
    #[command(subcommand)]
    command: Option<Command>,
    /// Show Rook's debug messages; `--verbose <subsystem>` (content,
    /// terrain, ...) shows everything that area logs.
    #[arg(long, global = true, value_name = "SUBSYSTEM", num_args = 0..=1, require_equals = true, default_missing_value = "")]
    verbose: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Open a place in the player; `rook <place>` is the same.
    ///
    /// Everything after `play` goes to `rook-player`, `--help` too.
    #[command(disable_help_flag = true)]
    Play {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Print the newest log file's path, or open the logs folder.
    Logs {
        /// Open the folder instead.
        #[arg(long)]
        open: bool,
    },
    /// Save or forget your Open Cloud key, which Rook tries for assets
    /// Roblox's asset delivery refuses anonymously.
    ///
    /// It is kept in the system's credential store (Windows Credential
    /// Manager, the macOS Keychain, the Secret Service on Linux), sent only
    /// to Roblox and never printed; `ROOK_OPEN_CLOUD_API_KEY` wins over it.
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// Show the download cache's folder and size, or clean it.
    ///
    /// The cache is `cache` in the data folder (`%LOCALAPPDATA%\Rook\cache`
    /// on Windows). Rook keeps it under the `cacheMaxSize` setting (2GB by
    /// default) itself, dropping what was used least recently.
    Cache {
        #[command(subcommand)]
        action: Option<CacheAction>,
    },
}

#[derive(Debug, Subcommand)]
enum KeyAction {
    /// Save the key, typed at a prompt that doesn't show it, or piped in.
    Set,
    /// Forget the saved key.
    Clear,
}

#[derive(Debug, Subcommand)]
enum CacheAction {
    /// Delete what hasn't been used for this many days.
    Prune {
        #[arg(long, value_name = "DAYS")]
        older_than: u64,
    },
    /// Delete what was used least recently until the cache is under
    /// its size limit (what Rook does by itself when it starts).
    Trim,
    /// Delete everything.
    Clear,
}

fn main() -> anyhow::Result<()> {
    rook_platform::console::close_unshared();
    let args: Vec<OsString> = std::env::args_os().collect();
    if let Some(play) = play::shortcut(&args) {
        return play;
    }
    let args = args.into_iter().map(|a| a.to_string_lossy().into_owned());
    let cli = Cli::parse_from(rook_engine::logging::fix_verbose_args(args));
    rook_engine::logging::init(&rook_engine::logging::Options {
        program: "rook",
        verbose: cli.verbose.clone(),
        file: false,
    });
    let Some(command_given) = cli.command else {
        Cli::command().print_help()?;
        return Ok(());
    };
    rook_engine::logging::run(|| command(command_given), |_| {})
}

fn command(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Play { args } => play::run(&args),
        Command::Logs { open } => logs::run(open),
        Command::Key { action } => match action {
            KeyAction::Set => key::set(),
            KeyAction::Clear => key::clear(),
        },
        Command::Cache { action } => cache::cache(match action {
            None => cache::CacheCommand::Show,
            Some(CacheAction::Prune { older_than }) => cache::CacheCommand::Prune(older_than),
            Some(CacheAction::Trim) => cache::CacheCommand::Trim,
            Some(CacheAction::Clear) => cache::CacheCommand::Clear,
        }),
    }
}
