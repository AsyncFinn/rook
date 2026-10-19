//! `rook-player`: open a place in a window.
//!
//! Loads a place, starts a play session with one local player (or the
//! server alone with `--run`), and draws what that player's client sees. Hold the right mouse button to look, WASD to move, Q/E to go
//! down/up, Shift to move faster, and the mouse wheel to change speed.
//! `--screenshot out.png` renders one frame offscreen instead, and
//! `--record-audio out.wav` mixes the first `--seconds` to a file.
//!
//! In Play mode the server runs on its own thread at 60 Hz, so the window
//! waits only on its client and the renderer. `--uncapped` draws as fast
//! as it can instead of at the monitor's refresh rate; the window title
//! shows frame times (`--frame-times` prints them too).
//!
//! With the `repl` feature (default) and an interactive terminal, the
//! terminal shows a `> ` prompt that runs Luau in the open place.

mod app;
mod bench;
mod game;
mod record;
mod scene;
mod server;
mod stats;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;
use game::Game;
use glam::{EulerRot, Mat3, Vec3};
use rook_content::decode::image::Image;
use rook_content::decode::mesh::Mesh;
use rook_content::{ContentRef, Resolver};
use rook_engine::{DataModel, InputEvent, MessageType, console};
use rook_render::{Camera, Renderer};
use rook_types::{CFrame, Value};
use scene::{SceneCache, SceneChange};

use app::camera::*;
use app::launch::*;
use app::readout::*;
use app::*;
use bench::*;
use scene::installed::*;

/// Simulation step: 60 Hz, like Roblox's frame-based scheduler.
const STEP: f64 = 1.0 / 60.0;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, CursorIcon, Window, WindowId};

#[derive(Debug, Parser)]
#[command(name = "rook-player", version, about = "Open a Roblox place in Rook")]
struct Args {
    /// Place file (.rbxl or .rbxlx).
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
    /// Also run this Luau file as a server script.
    #[arg(long)]
    script: Option<PathBuf>,
    /// Show the server only, without a player (Studio's Run mode).
    #[arg(long)]
    run: bool,
    /// Run this many frames offscreen and print where the time goes.
    #[arg(long)]
    bench: Option<u32>,
    /// Present frames as fast as they're drawn instead of at the monitor's
    /// refresh rate.
    #[arg(long)]
    uncapped: bool,
    /// Print frame times to the terminal every second, and a summary at exit.
    #[arg(long)]
    frame_times: bool,
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

/// The scene for a DataModel as it is now. Its pending render changes are
/// already drawn, so they're dropped (otherwise the first frame would
/// rebuild the whole scene again: a new DataModel's changes say "rebuild
/// everything"), all but scripts' effect calls, which aren't in the tree.
fn scene_of(dm: &mut DataModel, content: &Arc<Resolver>, camera: Vec3) -> SceneCache {
    let calls = dm.take_render_changes().effect_calls;
    let dom = &dm.runtime().world().dom;
    let mut scene = SceneCache::new(dom, content.clone(), camera);
    scene.take_effect_calls(dom, &calls);
    scene
}

/// Bring a cached scene up to date with a DataModel's changes and the mesh
/// files and images that finished loading since the last frame, then the
/// renderer with the scene. Returns whether anything changed.
fn refresh(
    cache: &mut SceneCache,
    dm: &mut DataModel,
    renderer: &mut Renderer,
    camera: Vec3,
) -> bool {
    let changes = dm.take_render_changes();
    let dom = &dm.runtime().world().dom;
    let change = if changes.is_empty() {
        SceneChange::None
    } else {
        cache.apply(dom, &changes)
    };
    cache.follow_camera(dom, camera);
    let mut loaded = cache.poll_files(dom, false);
    loaded.extend(cache.update_grass(camera));
    let change = match change {
        // A skybox image arrived: the lighting alone changed.
        SceneChange::None if loaded.is_empty() && cache.take_lighting_change() => {
            SceneChange::Parts {
                changed: Vec::new(),
                moved: Vec::new(),
            }
        }
        _ if loaded.is_empty() => change,
        SceneChange::None => SceneChange::Parts {
            changed: loaded,
            moved: Vec::new(),
        },
        SceneChange::Parts { mut changed, moved } => {
            changed.extend(loaded);
            SceneChange::Parts { changed, moved }
        }
        SceneChange::Full => SceneChange::Full,
    };
    match &change {
        SceneChange::None => return false,
        SceneChange::Parts { changed, moved } => {
            renderer.update_parts(&cache.scene, changed);
            renderer.move_parts(&cache.scene, moved);
        }
        SceneChange::Full => renderer.set_scene(&cache.scene),
    }
    for mesh in cache.take_dropped_meshes() {
        renderer.drop_mesh(mesh);
    }
    true
}

/// Get a loaded game ready to show: run `--seconds`, pick the camera,
/// build the scene (files near the camera load first) and say what
/// loaded.
fn prepare(game: &mut Game, args: &Args, content: &Arc<Resolver>) -> (SceneCache, Camera) {
    let load_time = game.load_start.elapsed().as_secs_f64();
    if args.seconds > 0.0 {
        game.run_for(args.seconds);
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
    }
    if let Some(fov) = args.fov {
        camera.fov_y = fov;
    }
    let scene = scene_of(game.view_mut(), content, camera.position);
    let name = args.place.file_name().unwrap_or_default().to_string_lossy();
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

/// Put the GPU in the log and in any crash report.
fn note_gpu(renderer: &Renderer) {
    rook_engine::logging::set_context("GPU", renderer.gpu().to_owned());
    tracing::debug!(target: "render", "GPU: {}", renderer.gpu());
}

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "tracy")]
    profiling::tracy_client::Client::start();
    let args = Args::parse_from(rook_engine::logging::fix_verbose_args(std::env::args()));
    rook_engine::logging::init(rook_engine::logging::Options {
        program: "rook-player",
        verbose: args.verbose.clone(),
        file: true,
    });
    rook_engine::logging::run(|| play(args))
}

fn play(args: Args) -> anyhow::Result<()> {
    // Asset delivery is on in the player, which exists to show places;
    // ROOK_ASSET_DELIVERY=off or settings.json turns it off.
    let content = Arc::new(Resolver::new(rook_engine::settings::content_config(true)));
    let install = Installed::find(&content);
    let script = match &args.script {
        Some(path) => Some(fs_err::read_to_string(path)?),
        None => None,
    };
    // A recording waits for each sound to load, so it keeps to game time
    // however fast it runs.
    let wait_for_sounds = args.record_audio.is_some();
    let windowed = args.record_audio.is_none() && args.bench.is_none() && args.screenshot.is_none();
    if windowed && !args.crash_test {
        // The window opens at once and shows a loading screen while the
        // place loads, as Roblox's does.
        let loading = game::Loading::begin(
            &args.place,
            !args.run,
            content.clone(),
            script,
            wait_for_sounds,
        )?;
        let mut launch = Launch::new(args, content, install, loading);
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop.run_app(&mut launch)?;
        return launch.finish();
    }
    let mut game = Game::load(
        &args.place,
        !args.run,
        content.clone(),
        script,
        wait_for_sounds,
    )?;
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
        return bench(
            &mut game,
            scene,
            frames,
            &camera,
            args.width,
            args.height,
            &install,
        );
    }

    if let Some(out) = args.screenshot {
        // A screenshot isn't a frame: it waits for the place's mesh files
        // and images, and their uploads.
        let dom = &game.view().runtime().world().dom;
        // Images on a mesh are asked for once its file has loaded.
        while !scene.poll_files(dom, true).is_empty() {}
        scene.update_grass(camera.position);
        let mut renderer = Renderer::headless()?;
        note_gpu(&renderer);
        install.apply(&mut renderer);
        renderer.set_scene(&scene.scene);
        renderer.finish_uploads();
        // Effects as they look once going: Smoke's puffs live 5 seconds.
        renderer.settle_particles(6.0);
        let pixels = renderer.render_to_image(args.width, args.height, &camera)?;
        image::save_buffer(
            &out,
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
