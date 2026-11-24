//! `cargo xtask`: Rook's developer tools, which never ship: looking inside
//! places and running them headless, measuring Studio, checking the
//! repository and making builds for other people. What ships is `rook`
//! (apps/rook-cli), which opens places.

mod commands;

use commands::{
    conformance, content, core_scripts, corpus, drift, dump, gen_api, package, pin_content,
    roundtrip, run_headless, studio_defaults, terrain, vendor_packages,
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
        /// A player joins, and their client runs too; its output and the
        /// members it lacks are printed after the server's.
        #[arg(long)]
        join: bool,
        /// The joined client runs Roblox's CoreScripts from the Studio
        /// install (the `coreScripts` setting, in builds with developer
        /// overrides).
        #[arg(long = rook_engine::settings::CORE_SCRIPTS.long(), requires = "join")]
        core_scripts: bool,
        /// With `--join`: print the client's tree from this instance at the
        /// end (names by dots, as `CoreGui.RobloxGui`), with each GUI
        /// object's size and whether it shows.
        #[arg(long, requires = "join")]
        dump: Option<String>,
        /// With `--join`: after `--seconds`, the player clicks here (`x,y`
        /// in pixels of a 1280 × 720 window); repeat for more clicks.
        #[arg(long, requires = "join")]
        click: Vec<rook_engine::ScreenPoint>,
        /// With `--join`: the player's settings (`UserSettings().GameSettings`)
        /// load from this file, as rook-player's `GlobalBasicSettings.xml`,
        /// and are saved to it at the end.
        #[arg(long, requires = "join")]
        user_settings: Option<PathBuf>,
        /// With `--join`: after the clicks, the player presses this key (an
        /// `Enum.KeyCode` name: `One`, `Backquote`); repeat for more keys.
        #[arg(long, requires = "join")]
        key: Vec<String>,
        /// With `--join`: a pad of this kind (`xbox`, `ps4`, `ps5`,
        /// `default`) is plugged in as Gamepad1 when the client starts, and `--key`s
        /// naming its buttons (`ButtonA`, `DPadUp`) press them on it.
        #[arg(long, requires = "join", value_parser = parse_gamepad)]
        gamepad: Option<rook_engine::GamepadKind>,
        /// With `--join`: after the keys, the mouse wheel turns this many
        /// steps (positive is forward); repeat for more turns.
        #[arg(long, requires = "join", allow_hyphen_values = true)]
        wheel: Vec<f32>,
        /// After `--seconds`, keep running in real time with a `>` prompt
        /// that runs Luau on the server, until input ends (Ctrl+C on an
        /// empty line, or Ctrl+D).
        #[arg(long)]
        repl: bool,
    },
    /// Show how content references resolve: what each means, which
    /// provider has it (Rook's defaults, the Roblox install, content
    /// folders, the cache or asset delivery), and why one fails.
    ///
    /// Asset delivery is on here unless `--offline` or the
    /// `assetDelivery` setting is off; downloads go to the cache. Given a place
    /// or model file, reports every content reference in it and its
    /// outcome.
    Content {
        /// References to resolve (`rbxassetid://5`, `rbxasset://textures/face.png`, ...),
        /// or one place or model file.
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
    },
    /// Load a place or model, save it again, reload it, and report every
    /// difference: what proves saving keeps a file whole.
    Roundtrip {
        file: PathBuf,
        /// Save in this format instead of the file's own.
        #[arg(long, value_enum)]
        to: Option<FormatArg>,
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
    /// Copy the packages Rook's CoreScripts require (React-Lua and its
    /// dependencies) into crates/rook-engine/corescripts/Packages, at the
    /// pins in its packages.toml.
    VendorPackages,
    /// Build Rook's CoreScripts (crates/rook-engine/corescripts) into an
    /// .rbxm, the file the engine loads them from.
    CoreScripts {
        /// Where to write it: beside the exe that should load it. Without
        /// one there, a build reads crates/rook-engine/corescripts itself.
        #[arg(long, default_value = "target/release/RookCoreScripts.rbxm")]
        out: PathBuf,
    },
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
        Command::Dump {
            file,
            properties,
            depth,
        } => dump::run(&file, properties, depth),
        Command::Source { file, name } => dump::print_source(&file, &name),
        Command::Content {
            references,
            place,
            offline,
            refresh,
        } => {
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
        Command::RunHeadless {
            file,
            seconds,
            timeout,
            quiet,
            script,
            join,
            core_scripts,
            dump,
            click,
            user_settings,
            key,
            gamepad,
            wheel,
            repl,
        } => run_headless::run(
            &file,
            &run_headless::Run {
                seconds,
                timeout,
                quiet,
                script: script.as_deref(),
                join,
                core_scripts,
                dump: dump.as_deref(),
                clicks: &click,
                user_settings: user_settings.as_deref(),
                keys: &key,
                gamepad,
                wheel: &wheel,
                repl,
            },
        ),
        Command::Roundtrip { file, to } => roundtrip::run(&file, to.map(Into::into)),
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
        Command::VendorPackages => {
            vendor_packages::run(Path::new("crates/rook-engine/corescripts"))
        }
        Command::CoreScripts { out } => core_scripts::run(&out),
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

fn parse_gamepad(kind: &str) -> Result<rook_engine::GamepadKind, String> {
    use rook_engine::GamepadKind;
    Ok(match kind {
        "xbox" => GamepadKind::Xbox,
        "ps4" => GamepadKind::PlayStation4,
        "ps5" => GamepadKind::PlayStation5,
        "default" => GamepadKind::Default,
        _ => return Err("one of xbox, ps4, ps5, default".into()),
    })
}
