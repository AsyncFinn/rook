//! `rook-player`: open a place in a window.
//!
//! v0 is a viewer: it loads a place, draws its parts, and lets you fly
//! around. Hold the right mouse button to look, WASD to move, Q/E to go
//! down/up, Shift to move faster, and the mouse wheel to change speed.
//! `--screenshot out.png` renders one frame offscreen instead.

mod snapshot;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use clap::Parser;
use glam::{EulerRot, Mat3, Vec3};
use rook_engine::{MessageType, Options, Server};
use rook_render::{Camera, Renderer, Scene};
use rook_types::{CFrame, Value};

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
    /// Run the place's scripts for this many seconds before showing it.
    #[arg(long, default_value_t = 0.0)]
    seconds: f64,
    /// Also run this Luau file as a server script.
    #[arg(long)]
    script: Option<PathBuf>,
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

/// Load a place into a running server DataModel and start its scripts.
/// Rook plays places the way Studio's Run mode does: the server simulates
/// and the viewport shows it through an editor camera.
fn load(path: &PathBuf) -> anyhow::Result<Server> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut server = Server::load_place(&bytes, &Options::default())?;
    server.start();
    print_log(&server);
    Ok(server)
}

fn print_log(server: &Server) {
    for entry in server.take_log() {
        let tag = match entry.kind {
            MessageType::Error => "ERROR ",
            MessageType::Warning => "WARN  ",
            _ => "",
        };
        println!("[{:>8.2}] {tag}{}", entry.time, entry.message);
    }
}

fn scene_of(server: &Server) -> Scene {
    snapshot::build(&server.runtime().world().dom)
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut server = load(&args.place)?;
    if let Some(script) = &args.script {
        let source = std::fs::read_to_string(script)
            .with_context(|| format!("reading {}", script.display()))?;
        server
            .runtime()
            .run_source("ServerScriptService.RookScript", &source, None)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        print_log(&server);
    }
    if args.seconds > 0.0 {
        server.run_for(args.seconds);
        print_log(&server);
    }
    let scene = scene_of(&server);
    let mut camera = snapshot::initial_camera(&server.runtime().world().dom);
    eprintln!("{} parts", scene.parts.len());
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

    if let Some(out) = args.screenshot {
        let mut renderer = Renderer::headless()?;
        renderer.set_scene(&scene);
        let pixels = renderer.render_to_image(args.width, args.height, &camera)?;
        image::save_buffer(
            &out,
            &pixels,
            args.width,
            args.height,
            image::ColorType::Rgba8,
        )?;
        eprintln!("wrote {}", out.display());
        return Ok(());
    }

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        title: format!("Rook - {}", args.place.display()),
        scene,
        server,
        accumulator: 0.0,
        camera: FlyCamera::new(camera),
        gfx: None,
        keys: HashSet::new(),
        looking: false,
        last_frame: Instant::now(),
    };
    event_loop.run_app(&mut app)?;
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
    scene: Scene,
    server: Server,
    /// Real time not yet simulated, in seconds.
    accumulator: f64,
    camera: FlyCamera,
    gfx: Option<Gfx>,
    keys: HashSet<KeyCode>,
    looking: bool,
    last_frame: Instant,
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
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);
        let mut renderer = Renderer::new(device, queue, config.format);
        renderer.set_scene(&self.scene);
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
        if let Some(cam) = self.server.current_camera() {
            let c = &self.camera.camera;
            self.server.set_property(
                cam,
                "CFrame",
                Value::CFrame(CFrame {
                    position: c.position,
                    rotation: c.rotation,
                }),
            );
        }

        // Simulate in fixed 60 Hz steps, catching up at most a few per frame.
        self.accumulator += f64::from(dt);
        let mut steps = 0;
        while self.accumulator >= STEP && steps < 4 {
            self.server.step();
            self.accumulator -= STEP;
            steps += 1;
        }
        if steps == 4 {
            self.accumulator = 0.0;
        }
        print_log(&self.server);
        if self.server.take_render_dirty() {
            self.scene = scene_of(&self.server);
            if let Some(gfx) = &mut self.gfx {
                gfx.renderer.set_scene(&self.scene);
            }
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
            eprintln!("could not start the renderer: {e:#}");
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
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
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
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state,
                ..
            } => {
                self.looking = state == ElementState::Pressed;
                if let Some(gfx) = &self.gfx {
                    gfx.window.set_cursor_visible(!self.looking);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.camera.speed = (self.camera.speed * 1.2f32.powf(steps)).clamp(1.0, 2000.0);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;
                self.update(dt);
                self.draw();
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
