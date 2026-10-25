//! The window while a place loads: it opens at once and shows a loading
//! screen, as Roblox's does, then hands over to [`App`] once the game is
//! ready. A place file dropped on the window replaces the open place;
//! while none is open, the window says why ([`notice`]).

use crate::*;

/// The loading screen's color, a dark gray like Roblox's (linear).
const BACKGROUND: wgpu::Color = wgpu::Color {
    r: 0.012,
    g: 0.013,
    b: 0.016,
    a: 1.0,
};

/// The title while no place is open, after one failed to.
const WAITING: &str = "Rook - drop a place file here";

pub(crate) struct Launch {
    args: Args,
    setup: game::Setup,
    install: Option<Installed>,
    loading: Option<game::Loading>,
    /// The window, while no game draws in it.
    gfx: Option<Gfx>,
    app: Option<App>,
    /// The player's settings, while no game has them.
    player: PlayerSettings,
    /// The terminal command prompt, while no game has it.
    #[cfg(feature = "repl")]
    repl: Option<rook_engine::console::repl::Repl>,
    /// What the window says while no place is open or loading.
    notice: Option<notice::Notice>,
}

impl Launch {
    pub(crate) fn new(
        args: Args,
        setup: game::Setup,
        install: Installed,
        player: PlayerSettings,
    ) -> Self {
        #[cfg(feature = "repl")]
        let repl = if args.no_repl {
            None
        } else {
            rook_engine::console::repl::Repl::start()
        };
        let place = args.place.clone();
        let mut launch = Self {
            args,
            setup,
            install: Some(install),
            loading: None,
            gfx: None,
            app: None,
            player,
            #[cfg(feature = "repl")]
            repl,
            notice: None,
        };
        // The place loads while the window opens.
        match place {
            Some(place) => launch.open(place),
            None => launch.notice = Some(notice::Notice::drop_here(&launch.setup.content)),
        }
        launch
    }

    fn title(&self) -> String {
        match &self.args.place {
            Some(place) => format!("Rook - {}", place.display()),
            None => WAITING.to_owned(),
        }
    }

    fn loading_title(&self) -> String {
        format!("{}  |  Loading...", self.title())
    }

    /// The title before any game is there.
    fn window_title(&self) -> String {
        match self.loading {
            Some(_) => self.loading_title(),
            None => WAITING.to_owned(),
        }
    }

    /// After the window closes: the run's summary.
    pub(crate) fn finish(mut self) {
        if let Some(app) = &mut self.app {
            app.game.save_user_settings();
        }
        if let Some(app) = &self.app
            && rook_engine::settings::frame_times(self.args.frame_times)
        {
            app.readout.print_run(&app.game);
        }
    }

    /// Close the open place, if any, and load `path` in the same window.
    fn open(&mut self, path: PathBuf) {
        if let Some(app) = self.app.take() {
            self.close(app);
        }
        // A place still loading is given up.
        self.loading = None;
        self.notice = None;
        let loading = game::Loading::begin(&path, &self.setup);
        self.args.place = Some(path);
        if let Some(gfx) = &self.gfx {
            gfx.window.set_title(&self.loading_title());
        }
        match loading {
            Ok(loading) => self.loading = Some(loading),
            Err(e) => self.not_opened(e),
        }
    }

    /// Stop `app`'s game and free what it loaded, keeping the window, the
    /// player's settings and the command prompt for the next place.
    fn close(&mut self, mut app: App) {
        app.look(false);
        app.game.save_user_settings();
        let App {
            game,
            scene,
            audio,
            gfx,
            install,
            player,
            ime,
            #[cfg(feature = "repl")]
            repl,
            ..
        } = app;
        // Its server thread ends and its sounds stop.
        drop((game, scene, audio));
        if let Some(mut gfx) = gfx {
            if ime {
                gfx.window.set_ime_allowed(false);
            }
            gfx.window.set_cursor_visible(true);
            gfx.window.set_cursor(CursorIcon::Default);
            gfx.clear_renderer();
            self.gfx = Some(gfx);
        }
        self.install = Some(install);
        self.player = player;
        #[cfg(feature = "repl")]
        {
            self.repl = repl;
        }
    }

    /// Say why the place didn't open, and wait for another.
    fn not_opened(&mut self, e: anyhow::Error) {
        let name = self
            .args
            .place
            .as_deref()
            .map(place_name)
            .unwrap_or_default();
        let e = e.context(format!("Couldn't open {name}"));
        console::print_message(MessageType::Error, &format!("{e:#}"));
        if let Some(gfx) = &self.gfx {
            gfx.window.set_title(WAITING);
        }
        self.notice = Some(notice::Notice::failed(&self.setup.content, &e));
    }

    /// Show the loading screen or the notice, then see whether the game is
    /// ready.
    fn loading_frame(&mut self) {
        let Some(gfx) = &mut self.gfx else {
            return;
        };
        if let Some(frame) = gfx.next_frame() {
            let view = frame.texture.create_view(&Default::default());
            if let Some(notice) = &mut self.notice {
                let (width, height) = (gfx.config.width, gfx.config.height);
                notice.show(&mut gfx.renderer, width, height);
                gfx.renderer.render(&view, width, height, &notice::CAMERA);
            } else {
                clear(&gfx.renderer, &view);
            }
            gfx.window.pre_present_notify();
            gfx.renderer.queue.present(frame);
        }
        let Some(loaded) = self.loading.as_mut().and_then(game::Loading::poll) else {
            return;
        };
        self.loading = None;
        match loaded {
            Ok(game) => self.start(game),
            Err(e) => self.not_opened(e),
        }
    }

    /// The game is ready: make the [`App`] and give it the window.
    fn start(&mut self, mut game: Game) {
        let place = self.args.place.clone().expect("a place loaded");
        let (scene, camera) = prepare(&mut game, &place, &self.args, &self.setup.content);
        game.free_run();
        let args = &self.args;
        let mut app = App {
            title: self.title(),
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
            cursor_shown: cursor::Shown::default(),
            ime: false,
            last_frame: Instant::now(),
            pending_mouse: None,
            install: self.install.take().expect("one game at a time"),
            uncapped: rook_engine::settings::uncapped(args.uncapped),
            readout: Readout::new(rook_engine::settings::frame_times(args.frame_times)),
            // Only the window plays sound: headless runs, `--bench`,
            // `--screenshot` and `--record-audio` never open a device.
            // `--volume` holds until the player changes the volume.
            audio: rook_engine::settings::audio_device().map(|device| {
                let volume = args.volume.unwrap_or(self.player.master_volume);
                rook_audio::Output::new(device, volume.clamp(0.0, 1.0))
            }),
            player: self.player,
            #[cfg(feature = "repl")]
            repl: self.repl.take(),
        };
        if let Some(gfx) = self.gfx.take() {
            gfx.window.set_title(&app.title);
            app.attach(gfx);
        }
        self.app = Some(app);
    }
}

impl ApplicationHandler for Launch {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(app) = &mut self.app {
            return app.resumed(event_loop);
        }
        if self.gfx.is_none() {
            match Gfx::open(
                event_loop,
                &self.window_title(),
                rook_engine::settings::uncapped(self.args.uncapped),
            ) {
                Ok(gfx) => {
                    gfx.set_fullscreen(self.player.fullscreen);
                    self.gfx = Some(gfx);
                }
                Err(e) => no_renderer(event_loop, e),
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // One event per file: the last one dropped opens.
        if let WindowEvent::DroppedFile(path) = event {
            return self.open(path);
        }
        if let Some(app) = &mut self.app {
            return app.window_event(event_loop, id, event);
        }
        if let Some(gfx) = &self.gfx {
            icon::on_event(&gfx.window, &event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.physical_key == PhysicalKey::Code(KeyCode::Escape) =>
            {
                event_loop.exit()
            }
            WindowEvent::Resized(size) => {
                if let Some(gfx) = &mut self.gfx {
                    gfx.resize(size);
                }
            }
            WindowEvent::RedrawRequested => self.loading_frame(),
            _ => {}
        }
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        if let Some(app) = &mut self.app {
            app.device_event(event_loop, id, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        match &mut self.app {
            Some(app) => app.about_to_wait(event_loop),
            None => {
                if let Some(gfx) = &self.gfx {
                    gfx.window.request_redraw();
                }
            }
        }
    }
}

/// The loading screen: the window filled with its gray.
fn clear(renderer: &Renderer, target: &wgpu::TextureView) {
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("loading screen"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(BACKGROUND),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    renderer.queue.submit([encoder.finish()]);
}
