//! `rook-player`: open a place in a window.
//!
//! Loads a place, starts a play session with one local player (or the
//! server alone with `--run`), and draws what that player's client sees. Hold the right mouse button to look, WASD to move, Q/E to go
//! down/up, Shift to move faster, and the mouse wheel to change speed.
//! `--screenshot out.png` renders one frame offscreen instead.
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
mod camera;
mod files;
mod game;
mod installed;
mod keys;
mod readout;
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
use rook_content::image::Image;
use rook_content::mesh::Mesh;
use rook_content::{ContentRef, Resolver};
use rook_engine::{DataModel, InputEvent, MessageType, console};
use rook_render::{Camera, Renderer};
use rook_types::{CFrame, Value};
use scene::{SceneCache, SceneChange};

use app::*;
use bench::*;
use camera::*;
use installed::*;
use readout::*;

/// Simulation step: 60 Hz, like Roblox's frame-based scheduler.
const STEP: f64 = 1.0 / 60.0;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
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
/// already drawn, so they're dropped: otherwise the first frame would
/// rebuild the whole scene again (a new DataModel's changes say "rebuild
/// everything").
fn scene_of(dm: &mut DataModel, content: &Arc<Resolver>) -> SceneCache {
    dm.take_render_changes();
    SceneCache::new(&dm.runtime().world().dom, content.clone())
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
    let mut loaded = cache.poll_files(dom, false);
    loaded.extend(cache.update_grass(camera));
    let change = match change {
        // A skybox image arrived: the lighting alone changed.
        SceneChange::None if loaded.is_empty() && cache.take_lighting_change() => {
            SceneChange::Parts(Vec::new())
        }
        _ if loaded.is_empty() => change,
        SceneChange::None => SceneChange::Parts(loaded),
        SceneChange::Parts(mut parts) => {
            parts.extend(loaded);
            SceneChange::Parts(parts)
        }
        SceneChange::Full => SceneChange::Full,
    };
    match &change {
        SceneChange::None => return false,
        SceneChange::Parts(parts) => renderer.update_parts(&cache.scene, parts),
        SceneChange::Full => renderer.set_scene(&cache.scene),
    }
    for mesh in cache.take_dropped_meshes() {
        renderer.drop_mesh(mesh);
    }
    true
}

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "tracy")]
    profiling::tracy_client::Client::start();
    let args = Args::parse();
    let load_start = Instant::now();
    // Asset delivery is on in the player, which exists to show places;
    // ROOK_ASSET_DELIVERY=off turns it off.
    let content = Arc::new(Resolver::new(rook_content::Config::from_env(true)));
    let install = Installed::find(&content);
    let script = match &args.script {
        Some(path) => Some(fs_err::read_to_string(path)?),
        None => None,
    };
    let mut game = Game::load(&args.place, !args.run, content.clone(), script)?;
    let load_time = load_start.elapsed().as_secs_f64();
    if args.seconds > 0.0 {
        game.run_for(args.seconds);
    }
    let mut scene = scene_of(game.view_mut(), &content);
    // The camera the place was saved with (clients start with a fresh one).
    let mut camera = game.saved_camera;
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
        install.apply(&mut renderer);
        renderer.set_scene(&scene.scene);
        renderer.finish_uploads();
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

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    game.free_run();
    let mut app = App {
        title: format!("Rook - {}", args.place.display()),
        scene,
        game,
        accumulator: 0.0,
        clock: 0.0,
        camera: FlyCamera::new(camera),
        gfx: None,
        keys: HashSet::new(),
        looking: false,
        look_anchor: None,
        cursor: PhysicalPosition::new(0.0, 0.0),
        last_frame: Instant::now(),
        pending_mouse: None,
        install,
        uncapped: args.uncapped,
        readout: Readout::new(args.frame_times),
        // Only the window plays sound: headless runs, `--bench` and
        // `--screenshot` never open a device.
        audio: rook_audio::enabled_by_env().then(rook_audio::Output::new),
        #[cfg(feature = "repl")]
        repl: if args.no_repl {
            None
        } else {
            rook_engine::repl::Repl::start()
        },
    };
    event_loop.run_app(&mut app)?;
    if args.frame_times {
        app.readout.print_run(&app.game);
    }
        pointing: false,
    Ok(())
}
