//! `rook`: command-line tools for the Rook engine.

mod conformance;
mod content;
mod corpus;
mod corpus_run;
mod dump;
mod gen_members;
mod roundtrip;
mod run;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "rook", version, about = "Rook engine tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print a place or model file's instance tree.
    Dump {
        file: PathBuf,
        /// Also print each instance's properties.
        #[arg(long, short)]
        properties: bool,
        /// Maximum depth to print (default: unlimited).
        #[arg(long)]
        depth: Option<usize>,
    },
    /// Print the source of scripts whose full name contains NAME.
    Source { file: PathBuf, name: String },
    /// Load a file, save it again, reload, and report every difference.
    Roundtrip {
        file: PathBuf,
        /// Save in this format instead of the file's own.
        #[arg(long, value_enum)]
        to: Option<FormatArg>,
    },
    /// Scan a folder of places and models and report what they use.
    CorpusScan {
        /// Folder to scan recursively.
        dir: PathBuf,
        /// Output folder for corpus.json and summary.md.
        #[arg(long, default_value = "reports/corpus")]
        out: PathBuf,
        /// Worker threads for small files (default: up to 8).
        #[arg(long)]
        jobs: Option<usize>,
        /// Files larger than this many megabytes are processed one at a time.
        #[arg(long, default_value_t = 64)]
        large_mb: u64,
    },
    /// Run a place's server Scripts headless and print their output.
    RunHeadless {
        file: PathBuf,
        /// Seconds of game time to simulate.
        #[arg(long, default_value_t = 10.0)]
        seconds: f64,
        /// Script timeout in seconds (Roblox Studio's default is 10).
        #[arg(long, default_value_t = 10.0)]
        timeout: f64,
        /// Print only the summary.
        #[arg(long, short)]
        quiet: bool,
        /// Also run this Luau file as a server script after the place starts.
        #[arg(long)]
        script: Option<PathBuf>,
        /// After `--seconds`, keep running in real time with a `>` prompt
        /// that runs Luau on the server, until input ends (Ctrl+C on an
        /// empty line, or Ctrl+D).
        #[cfg(feature = "repl")]
        #[arg(long)]
        repl: bool,
    },
    /// Run every place in a folder headless and report how many run cleanly.
    CorpusRun {
        dir: PathBuf,
        #[arg(long, default_value = "reports/run")]
        out: PathBuf,
        /// Seconds of game time to simulate per place.
        #[arg(long, default_value_t = 10.0)]
        seconds: f64,
        /// Script timeout in seconds.
        #[arg(long, default_value_t = 1.0)]
        timeout: f64,
        #[arg(long)]
        jobs: Option<usize>,
    },
    /// Record conformance cases in Roblox Studio (through Studio's MCP
    /// server), or run them in Rook and compare.
    Conformance {
        #[command(subcommand)]
        action: ConformanceAction,
        /// The conformance folder.
        #[arg(long, default_value = "conformance", global = true)]
        dir: PathBuf,
        /// Only cases whose `group/name` contains this text.
        #[arg(long, global = true)]
        filter: Option<String>,
    },
    /// Show how content references resolve: what each means, which
    /// provider has it (Rook's defaults, the Roblox install, content
    /// folders, the cache or asset delivery), and why one fails.
    ///
    /// Asset delivery is on here unless `--offline` or
    /// ROOK_ASSET_DELIVERY=off; downloads go to the cache.
    Content {
        /// References to resolve (`rbxassetid://5`, `rbxasset://textures/face.png`, ...).
        #[arg(required_unless_present = "place")]
        references: Vec<String>,
        /// Resolve every content reference in this place or model instead.
        #[arg(long, conflicts_with = "references")]
        place: Option<PathBuf>,
        /// Don't use the network: only the install, folders and the cache.
        #[arg(long)]
        offline: bool,
        /// Forget cached answers for these references first.
        #[arg(long)]
        refresh: bool,
        /// With `--place`, list every reference, not just failures.
        #[arg(long, short)]
        verbose: bool,
    },
    /// Show the download cache's folder and size, or clean it.
    ///
    /// The cache is `ROOK_CACHE_DIR`, else `%TEMP%\Rook\cache` on Windows,
    /// `~/Library/Caches/Rook` on macOS and `$XDG_CACHE_HOME/rook`
    /// elsewhere. Rook keeps it under `ROOK_CACHE_MAX` (2GB by default)
    /// itself, dropping what was used least recently.
    Cache {
        #[command(subcommand)]
        action: Option<CacheAction>,
    },
    /// Regenerate crates/rook-reflection/data/members.json from a Full API dump.
    ///
    /// The dump must match the pinned Roblox version; Roblox-Client-Tracker
    /// keeps one per version.
    GenMembers {
        /// Path to Full-API-Dump.json.
        dump: PathBuf,
        /// Where the dump came from, recorded in the output.
        #[arg(long)]
        source: String,
        #[arg(long, default_value = "crates/rook-reflection/data/members.json")]
        out: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum CacheAction {
    /// Delete what hasn't been used for this many days.
    Prune {
        #[arg(long, value_name = "DAYS")]
        older_than: u64,
    },
    /// Delete what was used least recently until the cache is under
    /// `ROOK_CACHE_MAX` (what Rook does by itself when it starts).
    Trim,
    /// Delete everything.
    Clear,
}

#[derive(Debug, Subcommand)]
enum ConformanceAction {
    /// Run every case in a Studio playtest and save the results as expectations.
    Record,
    /// Run every case in Rook and compare with the expectations.
    Run,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum FormatArg {
    Binary,
    Xml,
}

impl From<FormatArg> for rook_formats::Format {
    fn from(f: FormatArg) -> Self {
        match f {
            FormatArg::Binary => Self::Binary,
            FormatArg::Xml => Self::Xml,
        }
    }
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Dump {
            file,
            properties,
            depth,
        } => dump::run(&file, properties, depth),
        Command::Source { file, name } => dump::print_source(&file, &name),
        Command::Roundtrip { file, to } => roundtrip::run(&file, to.map(Into::into)),
        Command::CorpusScan {
            dir,
            out,
            jobs,
            large_mb,
        } => corpus::run(&dir, &out, jobs, large_mb),
        Command::GenMembers { dump, source, out } => gen_members::run(&dump, &source, &out),
        Command::Content {
            references,
            place,
            offline,
            refresh,
            verbose,
        } => {
            let options = content::Options {
                offline,
                refresh,
                verbose,
            };
            match place {
                Some(file) => content::place(&file, &options),
                None => content::references(&references, &options),
            }
        }
        Command::Cache { action } => content::cache(match action {
            None => content::CacheCommand::Show,
            Some(CacheAction::Prune { older_than }) => content::CacheCommand::Prune(older_than),
            Some(CacheAction::Trim) => content::CacheCommand::Trim,
            Some(CacheAction::Clear) => content::CacheCommand::Clear,
        }),
        Command::Conformance {
            action,
            dir,
            filter,
        } => match action {
            ConformanceAction::Record => conformance::record(&dir, filter.as_deref()),
            ConformanceAction::Run => conformance::run(&dir, filter.as_deref()),
        },
        Command::CorpusRun {
            dir,
            out,
            seconds,
            timeout,
            jobs,
        } => corpus_run::run(&dir, &out, seconds, timeout, jobs),
        Command::RunHeadless {
            file,
            seconds,
            timeout,
            quiet,
            script,
            #[cfg(feature = "repl")]
            repl,
        } => run::run(
            &file,
            seconds,
            timeout,
            quiet,
            script.as_deref(),
            #[cfg(feature = "repl")]
            repl,
        ),
    }
}
