//! `rook-player`: open a place in a window.
//!
//! Loads a place, starts a play session with one local player (or the
//! server alone with `--run`), and draws what that player's client sees.
//! Hold the right mouse button to look, WASD to move, Q/E to go down/up,
//! Shift to move faster, and the mouse wheel to change speed.
//! `--screenshot out.png` renders one frame offscreen instead, and
//! `--record-audio out.wav` mixes the first `--seconds` to a file.
//!
//! In Play mode the server runs on its own thread at 60 Hz, so the window
//! waits only on its client and the renderer, at the player's
//! `FramerateCap`; the title shows frame times (`--frame-times` prints them).
//!
//! With the `repl` feature (default) and an interactive terminal, the
//! terminal shows a `> ` prompt that runs Luau in the open place.

mod alert;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use glam::Vec3;
use rook_client::window::winit::event_loop::EventLoop;
use rook_client::{Bench, Game, Setup, Start, place_name};
use rook_content::Resolver;
use rook_engine::{console, settings};
use rook_lua::MessageType;
use rook_scene::Installed;

/// The system allocator on Windows is slow for blocks of tens of
/// kilobytes once a big place has made its heap large (a fraction of a
/// millisecond each): mimalloc keeps the frame's allocations cheap.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug, Parser)]
#[command(name = "rook-player", version, about = "Open a Roblox place in Rook")]
struct Args {
    /// Place file (.rbxl, .rbxlx, .rbxm or .rbxmx).
    place: PathBuf,
    /// Render one frame to this PNG instead of opening a window.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    #[arg(long, default_value_t = 1280)]
    width: u32,
    #[arg(long, default_value_t = 720)]
    height: u32,
    /// Camera position as x,y,z (default: the camera saved in the place).
    #[arg(long, value_parser = parse_vec3, allow_hyphen_values = true)]
    camera: Option<Vec3>,
    /// Point the camera looks at, as x,y,z.
    #[arg(long, value_parser = parse_vec3, allow_hyphen_values = true)]
    look_at: Option<Vec3>,
    /// Vertical field of view in degrees (default: the saved camera's).
    #[arg(long)]
    fov: Option<f32>,
    /// Mix the place's sounds to this WAV file for `--seconds` (10 by
    /// default) instead of opening a window; prints how loud it got.
    #[arg(long)]
    record_audio: Option<PathBuf>,
    /// Run the place's scripts for this many seconds before showing it.
    #[arg(long, default_value_t = 0.0)]
    seconds: f64,
    /// After `--seconds`, the player clicks here (`x,y` in window pixels),
    /// half a second apart, a second before the shot; repeat for more. The
    /// shot shows the cursor where the last click left it.
    #[arg(long)]
    click: Vec<rook_engine::ScreenPoint>,
    /// Also run this Luau file as a server script.
    #[arg(long)]
    script: Option<PathBuf>,
    /// Also run this Luau file as a LocalScript of the player.
    #[arg(long, conflicts_with = "run")]
    client_script: Option<PathBuf>,
    /// Show the server only, without a player (Studio's Run mode).
    #[arg(long)]
    run: bool,
    /// Run this many frames offscreen and print where the time goes.
    #[arg(long)]
    bench: Option<u32>,
    /// With `--bench`: turn the camera this many degrees each frame, as
    /// someone looking around does (default: it holds still).
    #[arg(long, default_value_t = 0.0)]
    turn: f32,
    /// With `--bench`: fly the camera this many studs each frame the way
    /// it looks, level, as someone crossing a map does (default: it holds
    /// still).
    #[arg(long, default_value_t = 0.0)]
    fly: f32,
    /// Run Roblox's own CoreScripts (top bar, player list, chat...) from
    /// Roblox's packages or the Studio install (the `coreScripts` setting,
    /// in builds with developer overrides).
    #[arg(long = settings::CORE_SCRIPTS.long())]
    core_scripts: bool,
    /// Present frames as fast as they're drawn, whatever the player's
    /// `FramerateCap` says (the `uncapped` setting).
    #[arg(long = settings::UNCAPPED.long())]
    uncapped: bool,
    /// Pace frames by this `FramerateCap` instead of the player's: -1 at
    /// the display's rate, 0 uncapped, else at most this many a second
    /// (the `framerateCap` setting).
    #[arg(long = settings::FRAMERATE_CAP.long(), allow_hyphen_values = true)]
    framerate_cap: Option<i32>,
    /// Print frame times to the terminal every second, and a summary at
    /// exit (the `frameTimes` setting).
    #[arg(long = settings::FRAME_TIMES.long())]
    frame_times: bool,
    /// The master volume, 0 to 1, until the in-game menu changes it (the
    /// menu's volume is kept with the player's settings).
    #[arg(long)]
    volume: Option<f32>,
    /// Don't show the `>` command prompt in the terminal.
    #[cfg(feature = "repl")]
    #[arg(long)]
    no_repl: bool,
    /// Show every content failure with its instance, reference and
    /// reason; `--verbose <subsystem>` (content, terrain, render, ...)
    /// shows everything that area logs.
    #[arg(long, value_name = "SUBSYSTEM", num_args = 0..=1, require_equals = true, default_missing_value = "")]
    verbose: Option<String>,
    /// Panic once the place has loaded (checks crash reports).
    #[arg(long, hide = true)]
    crash_test: bool,
}

impl Args {
    /// Whether this run opens a window for a player, rather than making a
    /// file or a measurement.
    fn windowed(&self) -> bool {
        self.record_audio.is_none() && self.bench.is_none() && self.screenshot.is_none()
    }

    /// The `FramerateCap` the settings give over the player's.
    fn framerate_cap(&self) -> Option<i32> {
        settings::framerate_cap(self.uncapped.then_some(true), self.framerate_cap)
    }

    /// The `frameTimes` setting.
    fn frame_times(&self) -> bool {
        settings::FRAME_TIMES.switch(self.frame_times.then_some(true)) == Some(true)
    }

    fn start(&self) -> Start {
        Start {
            width: self.width,
            height: self.height,
            camera: self.camera,
            look_at: self.look_at,
            fov: self.fov,
            seconds: self.seconds,
            clicks: self.click.clone(),
        }
    }

    fn window(&self) -> rook_client::window::Options {
        rook_client::window::Options {
            place: self.place.clone(),
            start: self.start(),
            framerate_cap: self.framerate_cap(),
            frame_times: self.frame_times(),
            volume: self.volume,
            #[cfg(feature = "repl")]
            repl: !self.no_repl,
        }
    }
}

fn parse_vec3(s: &str) -> Result<Vec3, String> {
    let parts: Vec<f32> = s
        .split(',')
        .map(|p| p.trim().parse::<f32>().map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    match parts.as_slice() {
        [x, y, z] => Ok(Vec3::new(*x, *y, *z)),
        _ => Err("expected x,y,z".into()),
    }
}

fn main() -> ExitCode {
    rook_platform::console::close_unshared();
    #[cfg(feature = "tracy")]
    profiling::tracy_client::Client::start();
    let argv = rook_engine::logging::fix_verbose_args(std::env::args());
    let args = Args::parse_from(argv);
    rook_engine::logging::init(&rook_engine::logging::Options {
        program: "rook-player",
        verbose: args.verbose.clone(),
        file: true,
    });
    let windowed = args.windowed();
    let crashed = |message: &str| {
        if windowed {
            alert::show("Rook crashed", message);
        }
    };
    // The error prints inside `run`, which writes out everything printed
    // before it returns.
    let main = || match play(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Runs that make a file or a measurement come from a terminal
            // or a script, which a message box would hold up.
            if windowed {
                alert::error(&e);
            } else {
                console::print_message(MessageType::Error, &format!("{e:#}"));
            }
            ExitCode::FAILURE
        }
    };
    rook_engine::logging::run(main, crashed)
}

fn play(args: &Args) -> anyhow::Result<()> {
    // Asset delivery is on in the player, which exists to show places;
    // the setting turns it off.
    let content = Arc::new(Resolver::new(settings::content_config(true)));
    let install = Installed::find(&content);
    let read = |path: &Option<PathBuf>| path.as_ref().map(fs_err::read_to_string).transpose();
    let script = read(&args.script)?;
    let client_script = read(&args.client_script)?;
    let core_scripts = settings::core_scripts(args.core_scripts.then_some(true), &content);
    let mut setup = Setup {
        play: !args.run,
        content: content.clone(),
        script,
        client_script,
        // A recording waits for each sound to load, so it keeps to game
        // time however fast it runs.
        wait_for_sounds: args.record_audio.is_some(),
        core_scripts,
        user_settings: None,
    };
    if args.windowed() && !args.crash_test {
        // The player's settings, which only a window's player changes.
        setup.user_settings =
            rook_engine::data_folder::DataDir::locate().map(|d| d.user_settings());
        return rook_client::window::run(EventLoop::new()?, args.window(), setup, install);
    }
    let name = place_name(&args.place);
    let mut game =
        Game::load(&args.place, &setup).map_err(|e| e.context(format!("Couldn't open {name}")))?;
    if let Some(out) = &args.record_audio {
        let seconds = if args.seconds > 0.0 {
            args.seconds
        } else {
            10.0
        };
        return rook_client::record(&mut game, seconds, out);
    }
    let start = args.start();
    let (scene, camera) = rook_client::prepare(&mut game, &start, &content);

    if args.crash_test {
        panic!("--crash-test");
    }

    if let Some(frames) = args.bench {
        let how = Bench {
            turn: args.turn,
            fly: args.fly,
        };
        return rook_client::bench(&mut game, scene, frames, camera, &start, how, &install);
    }

    if let Some(out) = &args.screenshot {
        return rook_client::screenshot(&mut game, scene, camera, &start, &install, out);
    }

    unreachable!("the window opens above")
}
