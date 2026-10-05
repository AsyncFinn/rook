//! `raven`: command-line tools for the Raven engine.

mod corpus;
mod corpus_run;
mod dump;
mod gen_members;
mod roundtrip;
mod run;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "raven", version, about = "Raven engine tools")]
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
    /// Regenerate crates/raven-reflection/data/members.json from a Full API dump.
    ///
    /// The dump must match the pinned Roblox version; Roblox-Client-Tracker
    /// keeps one per version.
    GenMembers {
        /// Path to Full-API-Dump.json.
        dump: PathBuf,
        /// Where the dump came from, recorded in the output.
        #[arg(long)]
        source: String,
        #[arg(long, default_value = "crates/raven-reflection/data/members.json")]
        out: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum FormatArg {
    Binary,
    Xml,
}

impl From<FormatArg> for raven_formats::Format {
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
        } => run::run(&file, seconds, timeout, quiet),
    }
}
