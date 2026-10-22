//! `rook-tools`: command-line tools for the Rook engine.

mod commands;

use commands::{
    conformance, content, corpus, drift, dump, gen_members, logs, roundtrip, run, studio_defaults,
    terrain,
};
use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "rook-tools", version, about = "Rook engine tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Show Rook's debug messages; `--verbose <subsystem>` (content,
    /// terrain, ...) shows everything that area logs.
    #[arg(long, global = true, value_name = "SUBSYSTEM", num_args = 0..=1, require_equals = true, default_missing_value = "")]
    verbose: Option<String>,
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
    /// Show what a file's Terrain holds (smooth voxels or legacy blocks),
    /// or with `--scan`, count both across a folder's unique files.
    Terrain {
        /// Files to inspect, or one folder with `--scan`.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long)]
        scan: bool,
        /// Worker threads for `--scan` (default: 8).
        #[arg(long)]
        jobs: Option<usize>,
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
    /// ROOK_ASSET_DELIVERY=off; downloads go to the cache. Given a place
    /// or model file, reports every content reference in it and its
    /// outcome.
    Content {
        /// References to resolve (`rbxassetid://5`, `rbxasset://textures/face.png`, ...),
        /// or one place or model file.
        #[arg(required_unless_present_any = ["place", "pin"])]
        references: Vec<String>,
        /// Resolve every content reference in this place or model instead.
        #[arg(long, conflicts_with = "references")]
        place: Option<PathBuf>,
        /// Write the list of content Rook keeps for good again, with each
        /// asset's version today (crates/rook-content/data/kept.txt).
        #[arg(long, conflicts_with_all = ["references", "place"])]
        pin: bool,
        /// Don't use the network: only the install, folders and the cache.
        #[arg(long)]
        offline: bool,
        /// Forget cached answers for these references first.
        #[arg(long)]
        refresh: bool,
    },
    /// Print the newest log file's path, or open the logs folder.
    Logs {
        /// Open the folder instead.
        #[arg(long)]
        open: bool,
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
    /// Check the repository for drift (copied functions, long comment
    /// blocks, environment reads below rook-engine, broken doc links and
    /// indexes, British spellings); `scripts/check` runs it.
    #[command(hide = true)]
    Drift,
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
    /// Measure in Studio (through its MCP server) the property defaults
    /// the reflection database leaves out, and regenerate Rook's table.
    ///
    /// Reads Edit mode, then a playtest client; Studio must be in Edit.
    /// ROOK_STUDIO picks the window, as for `conformance`.
    StudioDefaults {
        /// Only list the properties without a database default.
        #[arg(long)]
        list: bool,
        #[arg(
            long,
            default_value = "crates/rook-dom/src/properties/studio_defaults.rs"
        )]
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
    let cli = Cli::parse_from(rook_engine::logging::fix_verbose_args(std::env::args()));
    // Long commands keep a log file; quick ones print and are done.
    let long = matches!(
        cli.command,
        Command::CorpusScan { .. } | Command::CorpusRun { .. } | Command::Conformance { .. }
    );
    rook_engine::logging::init(&rook_engine::logging::Options {
        program: "rook-tools",
        verbose: cli.verbose.clone(),
        file: long,
    });
    rook_engine::logging::run(|| command(cli.command))
}

fn command(command: Command) -> anyhow::Result<()> {
    match command {
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
        Command::StudioDefaults { list, out } => studio_defaults::run(list, &out),
        Command::Terrain { paths, scan, jobs } => {
            if scan {
                terrain::scan(&paths[0], jobs)
            } else {
                terrain::inspect(&paths)
            }
        }
        Command::Content {
            references,
            place,
            pin,
            offline,
            refresh,
        } => {
            if pin {
                return content::pin(std::path::Path::new("crates/rook-content/data/kept.txt"));
            }
            let options = content::Options { offline, refresh };
            // A lone file argument is a place or model.
            let place = place.or_else(|| match references.as_slice() {
                [one] if std::path::Path::new(one).is_file() => Some(PathBuf::from(one)),
                _ => None,
            });
            match place {
                Some(file) => content::place(&file, &options),
                None => content::references(&references, &options),
            }
        }
        Command::Logs { open } => logs::run(open),
        Command::Drift => drift::run(),
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
        } => corpus::runner::run(&dir, &out, seconds, timeout, jobs),
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
