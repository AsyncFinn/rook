//! `cargo xtask`: what works on the Rook repository itself and never ships:
//! measuring Studio, checking the repository and making builds for other
//! people. The tools that do ship are `rook` (apps/rook-cli).

mod commands;

use commands::{
    conformance, corpus, drift, gen_api, package, pin_content, studio_defaults, terrain,
};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Work on the Rook repository")]
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
    /// Build the folder to hand to other people, with `cargo dist`.
    Package,
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
    /// Write the list of content Rook keeps for good again, with each
    /// asset's version today (crates/rook-content/data/kept.txt).
    PinContent,
    /// Check the repository for drift (copied functions, long comment
    /// blocks, environment reads below rook-engine, broken doc links and
    /// indexes); `scripts/check` runs it.
    Drift,
    /// Update the API data (crates/rook-reflection/data/database.msgpack and
    /// members.json) to a new Roblox version from its Full API dump.
    ///
    /// Also takes the dump the current data was built from (members.json
    /// names it), to tell what Roblox changed. Roblox-Client-Tracker keeps
    /// one dump per version.
    GenApi {
        /// Path to the new version's Full-API-Dump.json.
        dump: PathBuf,
        /// Path to the Full-API-Dump.json the current data was built from.
        #[arg(long)]
        previous: PathBuf,
        /// The new Roblox version, as `major.minor.patch.build`.
        #[arg(long)]
        version: String,
        /// Where the dump came from, recorded in members.json.
        #[arg(long)]
        source: String,
        #[arg(long, default_value = "crates/rook-reflection/data/database.msgpack")]
        database: PathBuf,
        #[arg(long, default_value = "crates/rook-reflection/data/members.json")]
        members: PathBuf,
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
enum ConformanceAction {
    /// Run every case in a Studio playtest and save the results as expectations.
    Record,
    /// Run every case in Rook and compare with the expectations.
    Run,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse_from(rook_engine::logging::fix_verbose_args(std::env::args()));
    // Long commands keep a log file; quick ones print and are done.
    let long = matches!(
        cli.command,
        Command::CorpusScan { .. } | Command::CorpusRun { .. } | Command::Conformance { .. }
    );
    rook_engine::logging::init(&rook_engine::logging::Options {
        program: "xtask",
        verbose: cli.verbose.clone(),
        file: long,
    });
    rook_engine::logging::run(|| command(cli.command), |_| {})
}

fn command(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Package => package::run(),
        Command::CorpusScan {
            dir,
            out,
            jobs,
            large_mb,
        } => corpus::run(&dir, &out, jobs, large_mb),
        Command::Terrain { paths, scan, jobs } => {
            if scan {
                terrain::scan(&paths[0], jobs)
            } else {
                terrain::inspect(&paths)
            }
        }
        Command::CorpusRun {
            dir,
            out,
            seconds,
            timeout,
            jobs,
        } => corpus::runner::run(&dir, &out, seconds, timeout, jobs),
        Command::Conformance {
            action,
            dir,
            filter,
        } => match action {
            ConformanceAction::Record => conformance::record(&dir, filter.as_deref()),
            ConformanceAction::Run => conformance::run(&dir, filter.as_deref()),
        },
        Command::PinContent => pin_content::pin(Path::new("crates/rook-content/data/kept.txt")),
        Command::Drift => drift::run(),
        Command::GenApi {
            dump,
            previous,
            version,
            source,
            database,
            members,
        } => gen_api::run(
            &gen_api::Paths {
                dump: &dump,
                previous_dump: &previous,
                database: &database,
                members: &members,
            },
            &version,
            &source,
        ),
        Command::StudioDefaults { list, out } => studio_defaults::run(list, &out),
    }
}
