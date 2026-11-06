//! `rook`: open a place, and look inside places and Rook's own files.
//!
//! `rook place.rbxl` opens it in the player, as dragging it onto `rook.exe`
//! does, and with nothing to open it asks for a place. What works on the
//! repository itself is `cargo xtask` (apps/xtask).

mod commands;

use commands::{content, dump, key, logs, play, roundtrip, run};
use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "rook",
    version,
    about = "Open and inspect Roblox places with Rook"
)]
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
    /// Open a place in the player; `rook <place>` is the same.
    ///
    /// Everything after `play` goes to `rook-player`, `--help` too.
    #[command(disable_help_flag = true)]
    Play {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
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
    /// Load a file, save it again, reload, and report every difference.
    Roundtrip {
        file: PathBuf,
        /// Save in this format instead of the file's own.
        #[arg(long, value_enum)]
        to: Option<FormatArg>,
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
        /// With `--join`: after the keys, the mouse wheel turns this many
        /// steps (positive is forward); repeat for more turns.
        #[arg(long, requires = "join", allow_hyphen_values = true)]
        wheel: Vec<f32>,
        /// After `--seconds`, keep running in real time with a `>` prompt
        /// that runs Luau on the server, until input ends (Ctrl+C on an
        /// empty line, or Ctrl+D).
        #[cfg(feature = "repl")]
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
    rook_engine::logging::run(|| command(cli.command), |_| {})
}

fn command(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Play { args } => play::run(&args),
        Command::Dump {
            file,
            properties,
            depth,
        } => dump::run(&file, properties, depth),
        Command::Source { file, name } => dump::print_source(&file, &name),
        Command::Roundtrip { file, to } => roundtrip::run(&file, to.map(Into::into)),
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
        Command::Logs { open } => logs::run(open),
        Command::Key { action } => match action {
            KeyAction::Set => key::set(),
            KeyAction::Clear => key::clear(),
        },
        Command::Cache { action } => content::cache(match action {
            None => content::CacheCommand::Show,
            Some(CacheAction::Prune { older_than }) => content::CacheCommand::Prune(older_than),
            Some(CacheAction::Trim) => content::CacheCommand::Trim,
            Some(CacheAction::Clear) => content::CacheCommand::Clear,
        }),
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
            wheel,
            #[cfg(feature = "repl")]
            repl,
        } => run::run(
            &file,
            &run::Run {
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
                wheel: &wheel,
                #[cfg(feature = "repl")]
                repl,
            },
        ),
    }
}
