//! The window: input, the frame loop and drawing.

pub(crate) mod camera;
pub(crate) mod cursor;
pub(crate) mod frame_clock;
pub(crate) mod icon;
pub(crate) mod inbox;
pub(crate) mod keys;
pub(crate) mod launch;
pub(crate) mod mouse;
pub(crate) mod readout;

use crate::*;

pub(crate) struct Gfx {
    pub(crate) window: Arc<Window>,
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) renderer: Renderer,
}

pub(crate) struct App {
    pub(crate) title: String,
    pub(crate) scene: SceneCache,
    pub(crate) game: Game,
    /// Seconds since the window opened, which water and grass move by.
    pub(crate) clock: f64,
    pub(crate) camera: FlyCamera,
    pub(crate) gfx: Option<Gfx>,
    pub(crate) keys: HashSet<KeyCode>,
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
    /// What the cursor shows.
    pub(crate) cursor_shown: cursor::Shown,
    /// The platform's input method is on, while a TextBox has focus.
    pub(crate) ime: bool,
    /// What the renderer takes from the user's install.
    pub(crate) install: Installed,
    /// `--uncapped`, over the player's `FramerateCap`.
    pub(crate) uncapped: bool,
    pub(crate) readout: Readout,
    /// Where the game's sounds are heard; `None` with `ROOK_AUDIO=off`.
    pub(crate) audio: Option<rook_audio::Output<rook_dom::InstanceId>>,
    /// The player's settings the window last followed.
    pub(crate) player: PlayerSettings,
    /// The terminal command prompt, when the terminal is interactive.
    #[cfg(feature = "repl")]
    pub(crate) repl: Option<rook_engine::console::repl::Repl>,
}

/// What a frame draws: the camera as the frame's step left it. Only
/// [`App::update`] makes one, after stepping, and drawing reads the camera
/// from it alone, so no frame can draw a camera from before its step.
pub(crate) struct Drawn {
    camera: Camera,
}

/// How often the window draws: as the player's `FramerateCap` says, unless
/// `--uncapped`.
pub(crate) fn pacing(uncapped: bool, player: &PlayerSettings) -> Pacing {
    if uncapped {
        Pacing::Uncapped
    } else {
        player.pacing()
    }
}

fn present_mode(vsync: bool) -> wgpu::PresentMode {
    if vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
    }
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
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window.clone())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Renderer::device_descriptor(&adapter)))?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .context("surface is not supported by the adapter")?;
        let caps = surface.get_capabilities(&adapter);
        if let Some(srgb) = caps.formats.iter().find(|f| f.is_srgb()) {
            config.format = *srgb;
        }
        config.present_mode = present_mode(vsync);
        // One frame queued, not wgpu's default two: at the display's rate
        // each frame waits for the screen before it starts, so it shows the
        // mouse as it is then, rather than two or three refreshes late.
        config.desired_maximum_frame_latency = 1;
        surface.configure(&device, &config);
        let renderer = Renderer::new(&adapter, device, queue, config.format);
        crate::note_gpu(&renderer);
        Ok(Self {
            window,
            surface,
            config,
            renderer,
        })
    }

    /// Fill the screen, or go back to a window, as the player's
    /// `Fullscreen` setting says.
    pub(crate) fn set_fullscreen(&self, on: bool) {
        let mode = on.then_some(winit::window::Fullscreen::Borderless(None));
        self.window.set_fullscreen(mode);
    }

    /// Present at the display's rate, or as soon as each frame is drawn.
    fn set_vsync(&mut self, vsync: bool) {
        self.config.present_mode = present_mode(vsync);
        self.surface.configure(&self.renderer.device, &self.config);
    }

    /// Take a new window size.
    pub(crate) fn resize(&mut self, size: PhysicalSize<u32>) -> bool {
        if size.width == 0 || size.height == 0 {
            return false;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.renderer.device, &self.config);
        true
    }

    /// The next frame to draw on, or `None` when the surface had to be
    /// set up again.
    pub(crate) fn next_frame(&self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            _ => {
                self.surface.configure(&self.renderer.device, &self.config);
                None
            }
        }
    }
}

impl App {
    /// Draw the game in `gfx`'s window from now on.
    pub(crate) fn attach(&mut self, mut gfx: Gfx) {
        self.install.apply(&mut gfx.renderer);
        gfx.renderer.set_scene(&self.scene.scene);
        self.game.set_viewport(gfx.config.width, gfx.config.height);
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

    /// When the next frame may be drawn, under a frame rate cap.
    fn next_due(&self) -> Option<Instant> {
        match self.pacing() {
            Pacing::Cap(fps) => {
                Some(self.last_frame + Duration::from_secs_f64(1.0 / f64::from(fps)))
            }
            Pacing::Display | Pacing::Uncapped => None,
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
        self.flying = self.game.view().scripted_camera().is_none();
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
        if let Some(view) = self.game.view().scripted_camera() {
            self.flying = false;
            self.camera.follow(view);
        }
        if let Some(now) = self.game.player_settings()
            && now != self.player
        {
            self.follow(now);
        }
        if let Some(audio) = &mut self.audio {
            audio.sync(&self.game.sounds());
        }
        if let Some(gfx) = &mut self.gfx {
            let camera = self.camera.camera.position;
            self.scene
                .refresh(self.game.view_mut(), &mut gfx.renderer, camera);
        }
        Drawn {
            camera: self.camera.camera,
        }
    }

    /// Draw a frame; whether one was presented.
    pub(crate) fn draw(&mut self, drawn: &Drawn) -> bool {
        let Some(gfx) = &mut self.gfx else {
            return false;
        };
        let Some(frame) = gfx.next_frame() else {
            return false;
        };
        let view = frame.texture.create_view(&Default::default());
        // Wrapped so the shader's sines stay precise in long sessions.
        gfx.renderer.set_time((self.clock % 3600.0) as f32);
        gfx.renderer
            .render(&view, gfx.config.width, gfx.config.height, &drawn.camera);
        gfx.window.pre_present_notify();
        gfx.renderer.queue.present(frame);
        true
    }

    /// Step the game, then draw it; whether a frame was presented.
    fn frame(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let now = Instant::now();
        let elapsed = now - self.last_frame;
        let dt = self.frame_clock.step(elapsed.as_secs_f64()).min(0.1) as f32;
        self.last_frame = now;
        self.apply_inbox(event_loop);
        let drawn = self.update(dt);
        self.return_cursor();
        self.follow_mouse_lock();
        self.show_cursor();
        let presented = self.draw(&drawn);
        if let Some(title) = self.readout.frame(elapsed, &self.game)
            && let Some(gfx) = &self.gfx
        {
            gfx.window.set_title(&format!("{}  |  {title}", self.title));
        }
        profiling::finish_frame!();
        presented
    }
}

impl App {
    fn pacing(&self) -> Pacing {
        pacing(self.uncapped, &self.player)
    }

    /// Follow the settings the player changed: the volume, fullscreen,
    /// the frame rate cap. The camera reads its sensitivity as it turns.
    fn follow(&mut self, now: PlayerSettings) {
        let vsync = pacing(self.uncapped, &now) == Pacing::Display;
        if vsync != (self.pacing() == Pacing::Display)
            && let Some(gfx) = &mut self.gfx
        {
            gfx.set_vsync(vsync);
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

pub(crate) fn no_renderer(event_loop: &ActiveEventLoop, e: anyhow::Error) {
    let e = e
        .context(
            "Rook needs a graphics card that supports DirectX 12, Vulkan or Metal, \
             with up-to-date drivers.",
        )
        .context("Couldn't start drawing");
    crate::alert::error(&e);
    event_loop.exit();
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

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        match Gfx::open(event_loop, &self.title, self.pacing() == Pacing::Display) {
            Ok(gfx) => self.attach(gfx),
            Err(e) => no_renderer(event_loop, e),
        }
    }

    /// Events are only noted ([`inbox`]); the frame acts on them.
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if self.inbox.note(event) {
            self.redraw(event_loop);
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        self.inbox.note_device(&event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.loop_ran = true;
        let Some(gfx) = &self.gfx else {
            return;
        };
        // A hidden window is never asked to redraw: its first frame is
        // drawn here, and it shows once that frame is presented.
        if gfx.window.is_visible() == Some(false) {
            if self.frame(event_loop)
                && let Some(gfx) = &self.gfx
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
