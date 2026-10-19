//! The window while the place loads: it opens at once and shows a loading
//! screen, as Roblox's does, then hands over to [`App`] once the game is
//! ready.

use crate::*;

/// The loading screen's colour, a dark grey like Roblox's (linear).
const BACKGROUND: wgpu::Color = wgpu::Color {
    r: 0.012,
    g: 0.013,
    b: 0.016,
    a: 1.0,
};

pub(crate) struct Launch {
    args: Args,
    content: Arc<Resolver>,
    install: Option<Installed>,
    loading: Option<game::Loading>,
    /// The window, until the game is ready to draw in it.
    gfx: Option<Gfx>,
    app: Option<App>,
    failed: Option<anyhow::Error>,
}

impl Launch {
    pub(crate) fn new(
        args: Args,
        content: Arc<Resolver>,
        install: Installed,
        loading: game::Loading,
    ) -> Self {
        Self {
            args,
            content,
            install: Some(install),
            loading: Some(loading),
            gfx: None,
            app: None,
            failed: None,
        }
    }

    fn title(&self) -> String {
        format!("Rook - {}", self.args.place.display())
    }

    /// After the window closes: the load's error, or the run's summary.
    pub(crate) fn finish(self) -> anyhow::Result<()> {
        if let Some(e) = self.failed {
            return Err(e);
        }
        if let Some(app) = &self.app
            && self.args.frame_times
        {
            app.readout.print_run(&app.game);
        }
        Ok(())
    }

    /// Show the loading screen, then see whether the game is ready.
    fn loading_frame(&mut self, event_loop: &ActiveEventLoop) {
        let Some(gfx) = &mut self.gfx else {
            return;
        };
        if let Some(frame) = gfx.next_frame() {
            let view = frame.texture.create_view(&Default::default());
            let device = &gfx.renderer.device;
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loading screen"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BACKGROUND),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            gfx.renderer.queue.submit([encoder.finish()]);
            gfx.window.pre_present_notify();
            gfx.renderer.queue.present(frame);
        }
        let Some(loaded) = self.loading.as_mut().and_then(game::Loading::poll) else {
            return;
        };
        self.loading = None;
        match loaded {
            Ok(game) => self.start(game),
            Err(e) => {
                self.failed = Some(e);
                event_loop.exit();
            }
        }
    }

    /// The game is ready: make the [`App`] and give it the window.
    fn start(&mut self, mut game: Game) {
        let (scene, camera) = prepare(&mut game, &self.args, &self.content);
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
            pointing: false,
            last_frame: Instant::now(),
            pending_mouse: None,
            install: self.install.take().expect("started once"),
            uncapped: args.uncapped,
            readout: Readout::new(args.frame_times),
            // Only the window plays sound: headless runs, `--bench`,
            // `--screenshot` and `--record-audio` never open a device.
            audio: rook_audio::enabled_by_env().then(rook_audio::Output::new),
            #[cfg(feature = "repl")]
            repl: if args.no_repl {
                None
            } else {
                rook_engine::console::repl::Repl::start()
            },
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
            let title = format!("{}  |  Loading...", self.title());
            match Gfx::open(event_loop, &title, self.args.uncapped) {
                Ok(gfx) => self.gfx = Some(gfx),
                Err(e) => no_renderer(event_loop, &e),
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(app) = &mut self.app {
            return app.window_event(event_loop, id, event);
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
            WindowEvent::RedrawRequested => self.loading_frame(event_loop),
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
