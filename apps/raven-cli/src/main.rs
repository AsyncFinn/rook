//! `raven`: command-line tools for the Raven engine.

mod corpus;
mod dump;
mod roundtrip;

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
        Command::Roundtrip { file, to } => roundtrip::run(&file, to.map(Into::into)),
        Command::CorpusScan {
            dir,
            out,
            jobs,
            large_mb,
        } => corpus::run(&dir, &out, jobs, large_mb),
    }
}
