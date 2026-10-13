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

mod game;
mod keys;
mod meshes;
mod server;
mod snapshot;
mod stats;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;
use game::Game;
use glam::{EulerRot, Mat3, Vec3};
use rook_content::mesh::Mesh;
use rook_content::{ContentRef, Resolver};
use rook_engine::{DataModel, InputEvent, MessageType, console};
use rook_render::{Camera, Renderer};
use rook_types::{CFrame, Value};
use snapshot::{SceneCache, SceneChange};

/// Simulation step: 60 Hz, like Roblox's frame-based scheduler.
const STEP: f64 = 1.0 / 60.0;
use winit::application::ApplicationHandler;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

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

fn scene_of(dm: &DataModel, content: &Arc<Resolver>) -> SceneCache {
    SceneCache::new(&dm.runtime().world().dom, content.clone())
}

/// Bring a cached scene up to date with a DataModel's changes and the mesh
/// files that finished loading since the last frame, then the renderer
/// with the scene. Returns whether anything changed.
fn refresh(cache: &mut SceneCache, dm: &mut DataModel, renderer: &mut Renderer) -> bool {
    let changes = dm.take_render_changes();
    let dom = &dm.runtime().world().dom;
    let change = if changes.is_empty() {
        SceneChange::None
    } else {
        cache.apply(dom, &changes)
    };
    let loaded = cache.poll_meshes(dom, false);
    let change = match change {
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
    let head = installed_head(&content);
    let script = match &args.script {
        Some(path) => Some(fs_err::read_to_string(path)?),
        None => None,
    };
    let mut game = Game::load(&args.place, !args.run, content.clone(), script)?;
    let load_time = load_start.elapsed().as_secs_f64();
    if args.seconds > 0.0 {
        game.run_for(args.seconds);
    }
    let mut scene = scene_of(game.view(), &content);
    // The camera the place was saved with (clients start with a fresh one).
    let mut camera = game.saved_camera;
    let name = args.place.file_name().unwrap_or_default().to_string_lossy();
    console::print_message(
        MessageType::Info,
        &format!(
            "Loaded {name} in {load_time:.2} s ({} parts)",
            thousands(scene.scene.parts.len())
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
            frames,
            &camera,
            args.width,
            args.height,
            head.as_ref(),
            &content,
        );
    }

    if let Some(out) = args.screenshot {
        // A screenshot isn't a frame: it waits for the place's mesh files.
        scene.poll_meshes(&game.view().runtime().world().dom, true);
        let mut renderer = Renderer::headless()?;
        if let Some(head) = &head {
            renderer.set_head_mesh(head);
        }
        renderer.set_scene(&scene.scene);
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
        camera: FlyCamera::new(camera),
        gfx: None,
        keys: HashSet::new(),
        looking: false,
        last_frame: Instant::now(),
        pending_mouse: None,
        head,
        uncapped: args.uncapped,
        readout: Readout::new(args.frame_times),
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
    Ok(())
}

/// The Head MeshType's mesh from the user's Roblox install, read from disk
/// at startup; `None` (Rook draws its own) without an install or the file.
fn installed_head(content: &Resolver) -> Option<Mesh> {
    let fetched = content.resolve_local(&ContentRef::File("avatar/heads/head.mesh".into()))?;
    match Mesh::decode(&fetched.bytes) {
        Ok(mesh) => Some(mesh),
        Err(e) => {
            console::print_message(
                MessageType::Warning,
                &format!("{}: {e}; drawing Rook's own head", fetched.source.location),
            );
            None
        }
    }
}

/// `29436` → `29,436`.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Time each stage of a frame, offscreen.
fn bench(
    game: &mut Game,
    frames: u32,
    camera: &Camera,
    width: u32,
    height: u32,
    head: Option<&Mesh>,
    content: &Arc<Resolver>,
) -> anyhow::Result<()> {
    let mut renderer = Renderer::headless()?;
    if let Some(head) = head {
        renderer.set_head_mesh(head);
    }
    let mut cache = scene_of(game.view(), content);
    renderer.set_scene(&cache.scene);
    let (mut sim, mut build, mut render) = (0.0, 0.0, 0.0);
    let mut updates = 0;
    let secs = |t: Instant| t.elapsed().as_secs_f64();
    game.set_viewport(width, height);
    let mut mouse = 0.0;
    let mut whole = stats::Times::default();
    game.server_steps.clear();
    game.client_steps.clear();
    for frame in 0..frames {
        let frame_start = Instant::now();
        // A moving cursor, as when someone plays: each move casts the mouse ray.
        let t = Instant::now();
        let x = (frame % 100) as f32 / 100.0 * width as f32;
        game.input(InputEvent::MouseMove {
            x,
            y: height as f32 / 2.0,
        });
        mouse += secs(t);
        let t = Instant::now();
        game.step();
        sim += secs(t);
        let t = Instant::now();
        if refresh(&mut cache, game.view_mut(), &mut renderer) {
            updates += 1;
        }
        build += secs(t);
        let t = Instant::now();
        renderer.render_to_image(width, height, camera)?;
        render += secs(t);
        whole.push(frame_start.elapsed());
        profiling::finish_frame!();
    }
    game.print_logs();
    let ms = |total: f64| total / f64::from(frames) * 1000.0;
    eprintln!(
        "{frames} frames, {} parts, {} mesh files, scene updated on {updates} frames",
        cache.scene.parts.len(),
        cache.scene.meshes.len()
    );
    eprintln!(
        "per frame: mouse {:.2} ms, simulate {:.2} ms, scene update and upload {:.2} ms, render+readback {:.2} ms",
        ms(mouse),
        ms(sim),
        ms(build),
        ms(render)
    );
    eprintln!("server step: {}", game.server_steps.summary());
    if !game.client_steps.is_empty() {
        eprintln!("client step: {}", game.client_steps.summary());
    }
    eprintln!(
        "simulation step (server + client): {}",
        game.total_steps().summary()
    );
    eprintln!(
        "whole frame (offscreen, with readback): {}",
        whole.summary()
    );
    Ok(())
}

/// Yaw/pitch camera driven by the mouse and keyboard.
#[derive(Debug)]
struct FlyCamera {
    camera: Camera,
    yaw: f32,
    pitch: f32,
    /// Studs per second.
    speed: f32,
}

impl FlyCamera {
    fn new(camera: Camera) -> Self {
        let f = camera.forward();
        Self {
            yaw: (-f.x).atan2(-f.z),
            pitch: f.y.clamp(-1.0, 1.0).asin(),
            camera,
            speed: 40.0,
        }
    }

    fn update_rotation(&mut self) {
        self.pitch = self.pitch.clamp(-1.55, 1.55);
        self.camera.rotation = Mat3::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0);
    }
}

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

struct App {
    title: String,
    scene: SceneCache,
    game: Game,
    /// Real time not yet simulated, in seconds.
    accumulator: f64,
    camera: FlyCamera,
    gfx: Option<Gfx>,
    keys: HashSet<KeyCode>,
    looking: bool,
    last_frame: Instant,
    /// The latest cursor position not yet sent to the game. Moves are sent
    /// once per frame because each costs a mouse raycast.
    pending_mouse: Option<(f32, f32)>,
    /// The install's head mesh, if any (see `installed_head`).
    head: Option<Mesh>,
    /// Present without waiting for the display (`--uncapped`).
    uncapped: bool,
    readout: Readout,
    /// The terminal command prompt, when the terminal is interactive.
    #[cfg(feature = "repl")]
    repl: Option<rook_engine::repl::Repl>,
}

impl App {
    fn init_gfx(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title(&self.title)
                    .with_inner_size(winit::dpi::LogicalSize::new(1280, 720)),
            )?,
        );
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window.clone())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("rook"),
                ..Default::default()
            }))?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .context("surface is not supported by the adapter")?;
        let caps = surface.get_capabilities(&adapter);
        if let Some(srgb) = caps.formats.iter().find(|f| f.is_srgb()) {
            config.format = *srgb;
        }
        config.present_mode = if self.uncapped {
            wgpu::PresentMode::AutoNoVsync
        } else {
            wgpu::PresentMode::AutoVsync
        };
        surface.configure(&device, &config);
        let mut renderer = Renderer::new(device, queue, config.format);
        if let Some(head) = &self.head {
            renderer.set_head_mesh(head);
        }
        renderer.set_scene(&self.scene.scene);
        self.game.set_viewport(config.width, config.height);
        self.gfx = Some(Gfx {
            window,
            surface,
            config,
            renderer,
        });
        Ok(())
    }

    fn update(&mut self, dt: f32) {
        let mut dir = Vec3::ZERO;
        let rot = self.camera.camera.rotation;
        for (key, v) in [
            (KeyCode::KeyW, -rot.z_axis),
            (KeyCode::KeyS, rot.z_axis),
            (KeyCode::KeyA, -rot.x_axis),
            (KeyCode::KeyD, rot.x_axis),
            (KeyCode::KeyE, Vec3::Y),
            (KeyCode::Space, Vec3::Y),
            (KeyCode::KeyQ, -Vec3::Y),
        ] {
            if self.keys.contains(&key) {
                dir += v;
            }
        }
        let boost = if self.keys.contains(&KeyCode::ShiftLeft) {
            4.0
        } else {
            1.0
        };
        self.camera.camera.position += dir.normalize_or_zero() * self.camera.speed * boost * dt;

        // The editor camera drives Workspace.CurrentCamera, as Studio's does.
        if let Some(cam) = self.game.view().current_camera() {
            let c = &self.camera.camera;
            self.game.view().set_property(
                cam,
                "CFrame",
                Value::CFrame(CFrame {
                    position: c.position,
                    rotation: c.rotation,
                }),
            );
        }

        if let Some((x, y)) = self.pending_mouse.take() {
            self.game.input(InputEvent::MouseMove { x, y });
        }

        #[cfg(feature = "repl")]
        if let Some(repl) = &mut self.repl {
            let game = &self.game;
            repl.poll(game.has_client(), |side, line| game.repl(side, line));
        }

        // The client steps at 60 Hz, catching up at most a few per frame;
        // the server keeps its own pace on its thread.
        self.accumulator += f64::from(dt);
        let mut steps = 0;
        while self.accumulator >= STEP && steps < 4 {
            self.accumulator -= STEP;
            steps += 1;
        }
        if steps == 4 {
            self.accumulator = 0.0;
        }
        self.game.advance(steps);
        if let Some(gfx) = &mut self.gfx {
            refresh(&mut self.scene, self.game.view_mut(), &mut gfx.renderer);
        }
    }

    fn draw(&mut self) {
        let Some(gfx) = &mut self.gfx else {
            return;
        };
        let frame = match gfx.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => {
                gfx.surface.configure(&gfx.renderer.device, &gfx.config);
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        gfx.renderer.render(
            &view,
            gfx.config.width,
            gfx.config.height,
            &self.camera.camera,
        );
        gfx.window.pre_present_notify();
        gfx.renderer.queue.present(frame);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_none()
            && let Err(e) = self.init_gfx(event_loop)
        {
            console::print_message(
                MessageType::Error,
                &format!("Could not start the renderer: {e:#}"),
            );
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gfx) = &mut self.gfx
                    && size.width > 0
                    && size.height > 0
                {
                    gfx.config.width = size.width;
                    gfx.config.height = size.height;
                    gfx.surface.configure(&gfx.renderer.device, &gfx.config);
                    self.game.set_viewport(size.width, size.height);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pending_mouse = Some((position.x as f32, position.y as f32));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if !event.repeat
                        && let Some(name) = keys::roblox_key(code)
                    {
                        self.game.input(InputEvent::Key {
                            code: name.to_owned(),
                            down: event.state == ElementState::Pressed,
                        });
                    }
                    match event.state {
                        ElementState::Pressed => {
                            if code == KeyCode::Escape {
                                event_loop.exit();
                            }
                            self.keys.insert(code);
                        }
                        ElementState::Released => {
                            self.keys.remove(&code);
                        }
                    }
                }
            }
            WindowEvent::MouseInput { button, state, .. } => {
                let down = state == ElementState::Pressed;
                if let Some((x, y)) = self.pending_mouse.take() {
                    self.game.input(InputEvent::MouseMove { x, y });
                }
                let number = match button {
                    MouseButton::Left => Some(1),
                    MouseButton::Right => Some(2),
                    MouseButton::Middle => Some(3),
                    _ => None,
                };
                if let Some(button) = number {
                    self.game.input(InputEvent::MouseButton { button, down });
                }
                if button == MouseButton::Right {
                    self.looking = down;
                    if let Some(gfx) = &self.gfx {
                        gfx.window.set_cursor_visible(!self.looking);
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.camera.speed = (self.camera.speed * 1.2f32.powf(steps)).clamp(1.0, 2000.0);
                self.game.input(InputEvent::Wheel { delta: steps });
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let elapsed = now - self.last_frame;
                let dt = elapsed.as_secs_f32().min(0.1);
                self.last_frame = now;
                self.update(dt);
                self.draw();
                if let Some(title) = self.readout.frame(elapsed, &self.game)
                    && let Some(gfx) = &self.gfx
                {
                    gfx.window.set_title(&format!("{}  |  {title}", self.title));
                }
                profiling::finish_frame!();
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.looking
        {
            self.camera.yaw -= delta.0 as f32 * 0.003;
            self.camera.pitch -= delta.1 as f32 * 0.003;
            self.camera.update_rotation();
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(gfx) = &self.gfx {
            gfx.window.request_redraw();
        }
    }
}

/// Frame times as players feel them, over each second of play: in the
/// window title, and with `--frame-times` in the terminal too.
struct Readout {
    print: bool,
    frames: stats::Times,
    run: stats::Times,
    since: Instant,
    /// Step samples already counted in earlier seconds.
    server_seen: usize,
    client_seen: usize,
}

impl Readout {
    fn new(print: bool) -> Self {
        Self {
            print,
            frames: stats::Times::default(),
            run: stats::Times::default(),
            since: Instant::now(),
            server_seen: 0,
            client_seen: 0,
        }
    }

    /// Count a frame; once a second, the line to show.
    fn frame(&mut self, took: Duration, game: &Game) -> Option<String> {
        self.frames.push(took);
        // Only the exit summary needs every frame of the run.
        if self.print {
            self.run.push(took);
        }
        if self.since.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.since = Instant::now();
        let f = self.frames.summary();
        self.frames.clear();
        let server = game.server_steps.since(self.server_seen).summary();
        let client = game.client_steps.since(self.client_seen).summary();
        self.server_seen = game.server_steps.len();
        self.client_seen = game.client_steps.len();
        let line = format!(
            "{:.0} FPS, frame median {:.2} ms, slowest 1% {:.2} ms, longest {:.2} ms, {} over 16.7 ms  |  \
             steps: server median {:.2} ms (longest {:.2}), client median {:.2} ms (longest {:.2})",
            f.fps(),
            ms(f.median),
            ms(f.slowest_1pct),
            ms(f.longest),
            f.over_budget,
            ms(server.median),
            ms(server.longest),
            ms(client.median),
            ms(client.longest),
        );
        if self.print {
            console::print_message(MessageType::Info, &line);
        }
        Some(line)
    }

    /// The whole run's frame and step times.
    fn print_run(&self, game: &Game) {
        let frames = self.run.summary();
        console::print_message(
            MessageType::Info,
            &format!("frames: {:.0} FPS, {frames}", frames.fps()),
        );
        console::print_message(
            MessageType::Info,
            &format!("server steps: {}", game.server_steps.summary()),
        );
        if !game.client_steps.is_empty() {
            console::print_message(
                MessageType::Info,
                &format!("client steps: {}", game.client_steps.summary()),
            );
        }
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
