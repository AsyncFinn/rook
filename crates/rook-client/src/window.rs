//! winit is re-exported so an app reaches it through here and the deny
//! rule keeping it in this crate holds.

mod camera;
mod cursor;
mod frame_clock;
mod icon;
mod inbox;
mod keys;
mod launch;
mod mouse;
mod readout;

pub use winit;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rook_engine::user_settings::{Pacing, PlayerSettings};
use rook_engine::{InputEvent, MouseLock};
use rook_render::{Camera, WindowSurface};
use rook_scene::{Installed, SceneCache, ToRender};
use rustc_hash::FxHashSet;
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::KeyCode;
use winit::window::{Window, WindowId};

use crate::game::{Game, Loading, Setup};
use crate::render_thread::RenderThread;
use crate::start::Start;
use camera::FlyCamera;
use launch::Launch;
use readout::Readout;

#[derive(Debug, Clone)]
pub struct Options {
    pub place: PathBuf,
    pub start: Start,
    /// The `FramerateCap` the settings give, over the player's.
    pub framerate_cap: Option<i32>,
    /// Print frame times to the terminal every second, and a summary at
    /// exit.
    pub frame_times: bool,
    /// The master volume, 0 to 1, until the in-game menu changes it.
    pub volume: Option<f32>,
    #[cfg(feature = "repl")]
    pub repl: bool,
}

/// The place loads while the window opens, hidden until the game has
/// drawn into it, so no blank window waits on the load.
pub fn run(
    event_loop: EventLoop<()>,
    options: Options,
    setup: Setup,
    install: Installed,
) -> anyhow::Result<()> {
    let name = crate::place_name(&options.place);
    let loading = Loading::begin(&options.place, &setup)
        .map_err(|e| e.context(format!("Couldn't open {name}")))?;
    let player = PlayerSettings::load(setup.user_settings.as_deref());
    let mut launch = Launch::new(options, setup, install, loading, player);
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut launch)?;
    launch.finish()
}

/// The window and what draws into it.
pub(crate) struct Gfx {
    /// Dropped first: the thread hands the surface back to be dropped here.
    pub(crate) frames: RenderThread<WindowSurface>,
    pub(crate) window: Arc<Window>,
}

pub(crate) struct App {
    pub(crate) title: String,
    pub(crate) scene: SceneCache,
    pub(crate) game: Game,
    /// Seconds since the window opened, which water and grass move by.
    pub(crate) clock: f64,
    pub(crate) camera: FlyCamera,
    pub(crate) gfx: Option<Gfx>,
    pub(crate) keys: FxHashSet<KeyCode>,
    /// Flying the window's own camera: nothing in the game drives the
    /// current camera ([`rook_engine::DataModel::scripted_camera`]).
    pub(crate) flying: bool,
    /// Turning the flying camera with the right mouse button.
    pub(crate) looking: bool,
    /// How the window holds the mouse for the game.
    pub(crate) held: MouseLock,
    /// Where a held cursor is put back to once a frame after it moves,
    /// where the platform can only confine it (see [`App::hold`]).
    pub(crate) anchor: Option<PhysicalPosition<f64>>,
    /// The held cursor moved since it was last put back.
    pub(crate) strayed: bool,
    /// What the window was told since the last frame.
    pub(crate) inbox: inbox::Inbox,
    /// The cursor's last position in the window, as the game knows it.
    pub(crate) cursor: PhysicalPosition<f64>,
    pub(crate) last_frame: Instant,
    /// How far each frame steps the game.
    pub(crate) frame_clock: frame_clock::FrameClock,
    /// The monitor the window was on when its refresh rate was read, and
    /// whether frames were shown at its rate then.
    pub(crate) refresh_read: Option<(Option<winit::monitor::MonitorHandle>, bool)>,
    /// The event loop ran (`about_to_wait`) since the last redraw. A
    /// redraw without it comes from a loop that isn't winit's: Windows'
    /// own, while the window is dragged or resized.
    pub(crate) loop_ran: bool,
    /// The window has nothing to draw into (it's minimized). Presenting
    /// paces frames shown at the display's rate, so then nothing would.
    pub(crate) undrawable: bool,
    pub(crate) cursor_shown: cursor::Shown,
    /// The platform's input method is on, while a TextBox has focus.
    pub(crate) ime: bool,
    /// What the renderer takes from the user's install.
    pub(crate) install: Installed,
    /// The `FramerateCap` the settings give (`--uncapped`,
    /// `--framerate-cap`), over the player's.
    pub(crate) framerate_cap: Option<i32>,
    pub(crate) readout: Readout,
    /// Where the game's sounds are heard; `None` with `audioDevice` off.
    pub(crate) audio: Option<rook_audio::Output<rook_dom::InstanceId>>,
    /// The game had something to hear when the mixer was last told what
    /// plays: silence is told once, not every frame.
    pub(crate) heard: bool,
    /// The player's settings the window last followed.
    pub(crate) player: PlayerSettings,
    /// The terminal command prompt, when the terminal is interactive.
    #[cfg(feature = "repl")]
    pub(crate) repl: Option<rook_engine::console::repl::Repl>,
}

/// What a frame draws: the camera as the frame's step left it, and the
/// scene's changes from that step. Only [`App::update`] makes one, after
/// stepping, and drawing reads the camera and the changes from it alone,
/// so no frame can draw a camera or a scene from before its step, and the
/// two always come from the same step.
pub(crate) struct Drawn {
    camera: Camera,
    scene: Option<ToRender>,
}

/// How often the window draws: as the settings' `FramerateCap` says, else
/// the player's.
pub(crate) fn pacing(framerate_cap: Option<i32>, player: &PlayerSettings) -> Pacing {
    framerate_cap.map_or_else(|| player.pacing(), Pacing::from_cap)
}

impl Gfx {
    /// Open the window and its GPU device, presenting at the display's
    /// rate with `vsync`.
    pub(crate) fn open(
        event_loop: &ActiveEventLoop,
        title: &str,
        vsync: bool,
    ) -> anyhow::Result<Self> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title(title)
                    .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
                    // Shown once the game has drawn into it (`App::about_to_wait`).
                    .with_visible(false),
            )?,
        );
        icon::set(&window, window.scale_factor());
        let size = window.inner_size();
        let surface = WindowSurface::new(window.clone(), size.width, size.height, vsync)?;
        crate::note_gpu(surface.renderer());
        Ok(Self {
            frames: RenderThread::new(surface),
            window,
        })
    }

    /// Fill the screen, or go back to a window, as the player's
    /// `Fullscreen` setting says.
    pub(crate) fn set_fullscreen(&self, on: bool) {
        let mode = on.then_some(winit::window::Fullscreen::Borderless(None));
        self.window.set_fullscreen(mode);
    }
}

impl App {
    /// Draw the game in `gfx`'s window from now on.
    pub(crate) fn attach(&mut self, mut gfx: Gfx) {
        let surface = gfx.frames.get();
        self.install.apply(surface.renderer_mut());
        self.scene.show_all(surface.renderer_mut());
        let (width, height) = surface.size();
        self.game.set_viewport(width, height);
        self.gfx = Some(gfx);
        self.read_refresh();
    }

    /// Step frames by the display's refreshes while they're shown at its
    /// rate (on the monitor the window is on now). The rate is asked for
    /// only when the monitor or the pacing changed: Windows sends hundreds
    /// of moves while the window is dragged, and the rate is a slow driver
    /// query.
    fn read_refresh(&mut self) {
        let Some(gfx) = &self.gfx else {
            return;
        };
        #[expect(
            clippy::disallowed_methods,
            reason = "on attach, settings changes and moves, which the frame applies at most once"
        )]
        let monitor = gfx.window.current_monitor();
        let vsync = self.pacing() == Pacing::Display;
        let now = Some((monitor.clone(), vsync));
        if self.refresh_read == now {
            return;
        }
        self.refresh_read = now;
        #[expect(
            clippy::disallowed_methods,
            reason = "only when the monitor or pacing changed"
        )]
        let mhz = monitor
            .filter(|_| vsync)
            .and_then(|m| m.refresh_rate_millihertz());
        let hz = mhz.map(|mhz| f64::from(mhz) / 1000.0);
        self.frame_clock.set_refresh(hz);
    }

    /// When the next frame may start: under a frame rate cap, and no
    /// sooner than it needs to so that it's ready as the render thread
    /// finishes the frame in flight ([`render_thread::Pacer`]).
    fn next_due(&self) -> Option<Instant> {
        let capped = match self.pacing() {
            Pacing::Cap(_) => self.frame_period().map(|period| self.last_frame + period),
            Pacing::Display | Pacing::Uncapped => None,
        };
        // Nothing is shown, so nothing is gained by more frames than the
        // display shows.
        let undrawn = self.undrawable.then(|| {
            let refresh = self.frame_clock.refresh().unwrap_or(1.0 / 60.0);
            let period = Duration::from_secs_f64(refresh);
            self.last_frame + self.frame_period().map_or(period, |p| p.max(period))
        });
        let just_in_time = self.gfx.as_ref().and_then(|gfx| gfx.frames.start_next_at());
        capped.max(just_in_time).max(undrawn)
    }

    /// The time between frames they keep to: the frame rate cap's, or the
    /// display's refresh when shown at its rate; `None` uncapped.
    fn frame_period(&self) -> Option<Duration> {
        match self.pacing() {
            Pacing::Cap(fps) => Some(Duration::from_secs_f64(1.0 / f64::from(fps))),
            Pacing::Display => self.frame_clock.refresh().map(Duration::from_secs_f64),
            Pacing::Uncapped => None,
        }
    }

    /// Step the game by `dt` seconds with the input the frame applied;
    /// what to draw.
    pub(crate) fn update(&mut self, dt: f32) -> Drawn {
        let typing = self.game.typing();
        if typing != self.ime
            && let Some(gfx) = &self.gfx
        {
            gfx.window.set_ime_allowed(typing);
            self.ime = typing;
        }
        // Whether a script drives the camera is as the last step left it
        // (below): nothing between steps changes it.
        if self.flying {
            self.fly(dt, typing);
        }
        // A script took the camera while the player was turning it.
        if !self.flying && self.looking {
            self.look(false);
        }
        #[cfg(feature = "repl")]
        if let Some(repl) = &mut self.repl {
            let game = &self.game;
            repl.poll(game.has_client(), |side, line| game.repl(side, line));
        }

        // The client steps once a drawn frame; the server keeps its own
        // pace on its thread.
        self.clock += f64::from(dt);
        self.game.advance(f64::from(dt));
        // Drawn as this step left it: the camera script has just turned it
        // by this frame's mouse movement, and the character it follows
        // stands where this step put it.
        let view = self.game.view().scripted_camera();
        self.flying = view.is_none();
        if let Some(view) = view {
            self.camera.follow(view);
        }
        let changed = self
            .game
            .player_settings()
            .filter(|now| **now != self.player);
        if let Some(now) = changed.cloned() {
            self.follow(now);
        }
        if let Some(audio) = &mut self.audio {
            // In silence that the mixer has been told of there is nothing
            // to gather, and no reason to take its lock from the audio
            // thread.
            let heard = self.game.hears_anything();
            if heard || self.heard {
                audio.sync(&self.game.sounds());
            }
            self.heard = heard;
        }
        // The scene follows the step here, while the render thread may
        // still be drawing the frame before.
        let scene = self.gfx.is_some().then(|| {
            let camera = self.camera.camera.position;
            self.scene.update(self.game.view_mut(), camera)
        });
        Drawn {
            camera: self.camera.camera,
            scene,
        }
    }

    /// Hand a frame to the render thread, once it has finished the frame
    /// before: the scene's changes go into the renderer, with the cursor,
    /// then the thread records, submits and presents it while the next
    /// frame steps.
    pub(crate) fn draw(&mut self, drawn: Drawn) {
        let (look, at) = self.cursor_wanted();
        let period = self.frame_period();
        let Some(gfx) = &mut self.gfx else {
            return;
        };
        let surface = gfx.frames.hand_over(self.last_frame, period);
        if let Some(scene) = drawn.scene {
            scene.apply(&self.scene, surface.renderer_mut());
        }
        let cursor_drawn =
            (self.scene).show_cursor(self.game.view(), surface.renderer_mut(), &look, at);
        let (window, camera, clock) = (gfx.window.clone(), drawn.camera, self.clock);
        gfx.frames
            .send(move |surface| surface.draw(&camera, clock, || window.pre_present_notify()));
        self.show_system_cursor(&look, cursor_drawn);
    }

    /// Step the game, then hand it to the render thread to draw.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let elapsed = now - self.last_frame;
        let dt = self.frame_clock.step(elapsed.as_secs_f64()).min(0.1) as f32;
        self.last_frame = now;
        self.apply_inbox(event_loop);
        let drawn = self.update(dt);
        self.return_cursor();
        self.follow_mouse_lock();
        self.draw(drawn);
        if let Some(title) = self.readout.frame(elapsed, &self.game)
            && let Some(gfx) = &self.gfx
        {
            gfx.window.set_title(&format!("{}  |  {title}", self.title));
        }
        profiling::finish_frame!();
    }
}

impl App {
    fn pacing(&self) -> Pacing {
        pacing(self.framerate_cap, &self.player)
    }

    /// Follow the settings the player changed: the volume, fullscreen,
    /// the frame rate cap. The camera reads its sensitivity as it turns.
    fn follow(&mut self, now: PlayerSettings) {
        let vsync = pacing(self.framerate_cap, &now) == Pacing::Display;
        if vsync != (self.pacing() == Pacing::Display)
            && let Some(gfx) = &mut self.gfx
        {
            gfx.frames.get().set_vsync(vsync);
        }
        if now.master_volume != self.player.master_volume
            && let Some(audio) = &mut self.audio
        {
            audio.set_master_volume(now.master_volume);
        }
        if now.fullscreen != self.player.fullscreen
            && let Some(gfx) = &self.gfx
        {
            gfx.set_fullscreen(now.fullscreen);
        }
        self.player = now;
        self.read_refresh();
    }
}

pub(crate) fn no_renderer(e: anyhow::Error) -> anyhow::Error {
    e.context(
        "Rook needs a graphics card that supports DirectX 12, Vulkan or Metal, \
         with up-to-date drivers.",
    )
    .context("Couldn't start drawing")
}

impl App {
    /// The window asked to be drawn. While Windows drags or resizes it, it
    /// runs a loop of its own, where winit's (and `about_to_wait`, which
    /// asks for frames) doesn't run; only paints come through. So each
    /// frame asks for the next, and the game keeps running then.
    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let theirs = !std::mem::replace(&mut self.loop_ran, false);
        if let Some(due) = self.next_due() {
            let now = Instant::now();
            if now < due {
                if !theirs {
                    // Early: our loop wakes it when due.
                    return;
                }
                // Their loop has no wait to give: wait here.
                #[expect(
                    clippy::disallowed_methods,
                    reason = "Windows' drag loop has no timer to wait in"
                )]
                std::thread::sleep(due - now);
            }
        }
        self.frame(event_loop);
        if let Some(gfx) = &self.gfx {
            gfx.window.request_redraw();
        }
    }
}

impl App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        if self.gfx.is_none() {
            let gfx = Gfx::open(event_loop, &self.title, self.pacing() == Pacing::Display);
            self.attach(gfx.map_err(no_renderer)?);
        }
        Ok(())
    }

    /// Events are only noted ([`inbox`]); the frame acts on them.
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if self.inbox.note(event) {
            self.redraw(event_loop);
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: &DeviceEvent) {
        self.inbox.note_device(event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.loop_ran = true;
        let Some(gfx) = &self.gfx else {
            return;
        };
        // A hidden window is never asked to redraw: its first frame is
        // drawn here, and it shows once that frame is presented.
        if gfx.window.is_visible() == Some(false) {
            self.frame(event_loop);
            if let Some(gfx) = &mut self.gfx
                && gfx.frames.get().presented()
            {
                gfx.window.set_visible(true);
            }
            return;
        }
        if let Some(due) = self.next_due()
            && Instant::now() < due
        {
            event_loop.set_control_flow(ControlFlow::WaitUntil(due));
            return;
        }
        event_loop.set_control_flow(ControlFlow::Poll);
        gfx.window.request_redraw();
    }
}
