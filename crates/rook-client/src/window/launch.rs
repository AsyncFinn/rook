//! The window while the place loads: it is made hidden and shown once the
//! game has drawn its first frame, as Roblox's is, so no blank window
//! waits on the load. A place that fails to load ends the run with
//! [`Launch::finish`]'s error.

use winit::application::ApplicationHandler;

use super::*;
use crate::start::prepare;

pub(crate) struct Launch {
    options: Options,
    setup: Setup,
    install: Option<Installed>,
    loading: Option<Loading>,
    /// The window, until the game is ready to draw in it.
    gfx: Option<Gfx>,
    app: Option<App>,
    failed: Option<anyhow::Error>,
    /// The player's settings as saved, until the game is there.
    player: PlayerSettings,
}

impl Launch {
    pub(crate) fn new(
        options: Options,
        setup: Setup,
        install: Installed,
        loading: Loading,
        player: PlayerSettings,
    ) -> Self {
        Self {
            options,
            setup,
            install: Some(install),
            loading: Some(loading),
            gfx: None,
            app: None,
            failed: None,
            player,
        }
    }

    fn title(&self) -> String {
        let build = if rook_engine::settings::DEVELOPER_BUILD {
            " - Developer build"
        } else {
            ""
        };
        format!("Rook - {}{build}", self.options.place.display())
    }

    /// After the window closes: the load's error, or the run's summary.
    pub(crate) fn finish(mut self) -> anyhow::Result<()> {
        if let Some(e) = self.failed {
            return Err(e);
        }
        if let Some(app) = &mut self.app {
            app.game.save_user_settings();
        }
        if let Some(app) = &self.app
            && self.options.frame_times
        {
            app.readout.print_run(&app.game);
        }
        Ok(())
    }

    /// See whether the game is ready, and start it once it is.
    fn poll(&mut self, event_loop: &ActiveEventLoop) {
        let Some(loading) = &mut self.loading else {
            return;
        };
        let Some(loaded) = loading.poll() else {
            return;
        };
        let name = loading.name().to_owned();
        self.loading = None;
        match loaded {
            Ok(game) => self.start(game),
            Err(e) => self.fail(event_loop, e.context(format!("Couldn't open {name}"))),
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, e: anyhow::Error) {
        self.failed.get_or_insert(e);
        event_loop.exit();
    }

    /// The game is ready: make the [`App`] and give it the window.
    fn start(&mut self, mut game: Game) {
        let (scene, camera) = prepare(&mut game, &self.options.start, &self.setup.content);
        game.free_run();
        let options = &self.options;
        let flying = game.view().scripted_camera().is_none();
        let mut app = App {
            title: self.title(),
            scene,
            game,
            clock: 0.0,
            camera: FlyCamera::new(camera),
            gfx: None,
            keys: FxHashSet::default(),
            flying,
            looking: false,
            held: MouseLock::Free,
            anchor: None,
            strayed: false,
            inbox: Default::default(),
            cursor: PhysicalPosition::new(0.0, 0.0),
            cursor_shown: cursor::Shown::default(),
            ime: false,
            last_frame: Instant::now(),
            frame_clock: Default::default(),
            refresh_read: None,
            loop_ran: false,
            undrawable: false,
            install: self.install.take().expect("started once"),
            framerate_cap: options.framerate_cap,
            readout: Readout::new(options.frame_times),
            // Only the window plays sound: headless runs, `--bench`,
            // `--screenshot` and `--record-audio` never open a device.
            // `--volume` holds until the player changes the volume.
            audio: rook_engine::settings::audio_device(self.player.output_device.as_deref()).map(
                |device| {
                    let volume = options.volume.unwrap_or(self.player.master_volume);
                    rook_audio::Output::new(device, volume.clamp(0.0, 1.0))
                },
            ),
            heard: false,
            player: self.player.clone(),
            #[cfg(feature = "repl")]
            repl: if options.repl {
                rook_engine::console::repl::Repl::start()
            } else {
                None
            },
        };
        if let Some(gfx) = self.gfx.take() {
            app.attach(gfx);
        }
        self.app = Some(app);
    }
}

impl ApplicationHandler for Launch {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(app) = &mut self.app {
            if let Err(e) = app.resumed(event_loop) {
                self.fail(event_loop, e);
            }
            return;
        }
        if self.gfx.is_none() {
            let vsync = pacing(self.options.framerate_cap, &self.player) == Pacing::Display;
            match Gfx::open(event_loop, &self.title(), vsync) {
                Ok(gfx) => {
                    gfx.set_fullscreen(self.player.fullscreen);
                    self.gfx = Some(gfx);
                }
                Err(e) => self.fail(event_loop, no_renderer(e)),
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(app) = &mut self.app {
            app.window_event(event_loop, id, event);
        }
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        if let Some(app) = &mut self.app {
            app.device_event(event_loop, id, &event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.is_none() {
            // A hidden window is never asked to redraw, so the load is
            // polled here.
            self.poll(event_loop);
        }
        if let Some(app) = &mut self.app {
            app.about_to_wait(event_loop);
        }
    }
}
