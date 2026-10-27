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
mod app;
mod bench;
mod console_window;
mod game;
mod record;
mod server;
mod stats;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;
use game::Game;
use glam::{EulerRot, Mat3, Vec3};
use rook_content::Resolver;
use rook_engine::user_settings::{Pacing, PlayerSettings};
use rook_engine::{InputEvent, MouseLock, console};
use rook_lua::MessageType;
use rook_render::{Camera, Renderer};
use rook_types::{CFrame, Value};
use rook_view::SceneCache;

use app::camera::*;
use app::launch::*;
use app::readout::*;
use app::*;
use bench::*;
use rook_view::Installed;

/// The system allocator on Windows is slow for blocks of tens of
/// kilobytes once a big place has made its heap large (a fraction of a
/// millisecond each): mimalloc keeps the frame's allocations cheap.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Simulation step: 60 Hz, like Roblox's frame-based scheduler.
const STEP: f64 = 1.0 / 60.0;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    DeviceEvent, DeviceId, ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, CursorIcon, Window, WindowId};

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
    /// half a second apart, a second before the shot; repeat for more.
    #[arg(long)]
    click: Vec<rook_engine::ScreenPoint>,
    /// Also run this Luau file as a server script.
    #[arg(long)]
    script: Option<PathBuf>,
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
    /// Run Roblox's own CoreScripts (top bar, player list, chat...) from
    /// Roblox's packages or the Studio install, whatever the build's
    /// `coreScripts` says (a developer override).
    #[arg(long)]
    core_scripts: bool,
    /// Present frames as fast as they're drawn, whatever the player's
    /// `FramerateCap` says (a developer override).
    #[arg(long)]
    uncapped: bool,
    /// Print frame times to the terminal every second, and a summary at
    /// exit (a developer override).
    #[arg(long)]
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

    /// `--uncapped`, in builds with developer overrides.
    fn uncapped(&self) -> bool {
        rook_engine::settings::developer("--uncapped", self.uncapped.then_some(())).is_some()
    }

    /// `--frame-times`, in builds with developer overrides.
    fn frame_times(&self) -> bool {
        rook_engine::settings::developer("--frame-times", self.frame_times.then_some(())).is_some()
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

/// Get a loaded game ready to show: run `--seconds`, pick the camera,
/// build the scene (files near the camera load first) and say what
/// loaded.
fn prepare(game: &mut Game, args: &Args, content: &Arc<Resolver>) -> (SceneCache, Camera) {
    let load_time = game.load_start.elapsed().as_secs_f64();
    // GUI lays out in the window it's drawn in.
    game.set_viewport(args.width, args.height);
    if args.seconds > 0.0 {
        game.run_for(args.seconds);
    }
    for point in &args.click {
        for event in point.click() {
            game.input(event);
            game.run_for(0.15);
        }
    }
    if !args.click.is_empty() {
        game.run_for(1.0);
    }
    // The camera the place was saved with (clients start with a fresh one).
    let mut camera = game.saved_camera;
    if let Some(pos) = args.camera {
        let target = args.look_at.unwrap_or(pos + camera.forward());
        camera = Camera {
            fov_y: camera.fov_y,
            ..Camera::look_at(pos, target)
        };
    } else if let Some(target) = args.look_at {
        camera = Camera {
            fov_y: camera.fov_y,
            ..Camera::look_at(camera.position, target)
        };
    } else if let Some(view) = game.view().scripted_camera() {
        // What the window would show: the game's camera.
        camera = Camera {
            position: view.cframe.position,
            rotation: view.cframe.rotation,
            fov_y: view.fov,
        };
    }
    if let Some(fov) = args.fov {
        camera.fov_y = fov;
    }
    let scene = SceneCache::for_data_model(game.view_mut(), content, camera.position);
    let name = place_name(&args.place);
    let terrain = scene.terrain_meshes();
    let terrain = if terrain > 0 {
        format!(", terrain in {} meshes", thousands(terrain))
    } else {
        String::new()
    };
    console::print_message(
        MessageType::Info,
        &format!(
            "Loaded {name} in {load_time:.2} s ({} parts{terrain})",
            thousands(scene.scene.parts.len() - scene.terrain_meshes())
        ),
    );
    (scene, camera)
}

/// A place's file name, as the window and messages call it.
fn place_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Put the GPU in the log and in any crash report.
fn note_gpu(renderer: &Renderer) {
    rook_engine::logging::set_context("GPU", renderer.gpu().to_owned());
    tracing::debug!(target: "render", "GPU: {}", renderer.gpu());
}

fn main() -> ExitCode {
    console_window::close_unshared();
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
    let main = || match play(args) {
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

fn play(args: Args) -> anyhow::Result<()> {
    // Asset delivery is on in the player, which exists to show places;
    // the setting turns it off.
    let content = Arc::new(Resolver::new(rook_engine::settings::content_config(true)));
    let install = Installed::find(&content);
    let script = match &args.script {
        Some(path) => Some(fs_err::read_to_string(path)?),
        None => None,
    };
    let core_scripts =
        rook_engine::settings::core_scripts(args.core_scripts.then_some(true), &content);
    let mut setup = game::Setup {
        play: !args.run,
        content: content.clone(),
        script,
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
        // The place loads while the window opens, hidden until the game
        // has drawn into it.
        let loading = game::Loading::begin(&args.place, &setup)
            .with_context(|| format!("Couldn't open {}", place_name(&args.place)))?;
        let player = PlayerSettings::load(setup.user_settings.as_deref());
        let mut launch = Launch::new(args, setup, install, loading, player);
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop.run_app(&mut launch)?;
        return launch.finish();
    }
    let mut game = Game::load(&args.place, &setup)
        .with_context(|| format!("Couldn't open {}", place_name(&args.place)))?;
    if let Some(out) = &args.record_audio {
        let seconds = if args.seconds > 0.0 {
            args.seconds
        } else {
            10.0
        };
        return record::record(&mut game, seconds, out);
    }
    let (mut scene, camera) = prepare(&mut game, &args, &content);

    if args.crash_test {
        panic!("--crash-test");
    }

    if let Some(frames) = args.bench {
        return bench(&mut game, scene, frames, camera, &args, &install);
    }

    if let Some(out) = &args.screenshot {
        // A screenshot isn't a frame: it waits for the place's mesh files
        // and images, and their uploads.
        scene.load_all(game.view(), camera.position);
        // GUI laid out again around the images that arrived fires Changed,
        // and scripts answer it (Roblox's React GUI re-renders): a few
        // frames let that settle, as it would in a window, before the shot.
        game.run_for(0.25);
        scene.load_all(game.view(), camera.position);
        let mut renderer = Renderer::headless()?;
        note_gpu(&renderer);
        install.apply(&mut renderer);
        renderer.set_scene(&scene.scene);
        scene.show_gui(game.view(), &mut renderer);
        renderer.finish_uploads();
        // Effects as they look once going: Smoke's puffs live 5 seconds.
        renderer.settle_particles(6.0);
        let pixels = renderer.render_to_image(args.width, args.height, &camera)?;
        image::save_buffer(
            out,
            &pixels,
            args.width,
            args.height,
            image::ColorType::Rgba8,
        )?;
        console::print_message(
            MessageType::Info,
            &format!("Saved screenshot to {}", out.display()),
        );
        return Ok(());
    }

    unreachable!("the window opens above")
}
