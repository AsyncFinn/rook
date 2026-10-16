//! The window: input, the frame loop and drawing.

pub(crate) mod camera;
pub(crate) mod keys;
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
    /// Real time not yet simulated, in seconds.
    pub(crate) accumulator: f64,
    /// Seconds since the window opened, which water and grass move by.
    pub(crate) clock: f64,
    pub(crate) camera: FlyCamera,
    pub(crate) gfx: Option<Gfx>,
    pub(crate) keys: HashSet<KeyCode>,
    pub(crate) looking: bool,
    /// Where the cursor was when looking started; it stays there (see
    /// [`App::look`]).
    pub(crate) look_anchor: Option<PhysicalPosition<f64>>,
    /// The cursor's last position in the window.
    pub(crate) cursor: PhysicalPosition<f64>,
    pub(crate) last_frame: Instant,
    /// The latest cursor position not yet sent to the game. Moves are sent
    /// once per frame because each costs a mouse raycast.
    pub(crate) pending_mouse: Option<(f32, f32)>,
    /// The cursor shows a pointing hand, over a ClickDetector.
    pub(crate) pointing: bool,
    /// What the renderer takes from the user's install.
    pub(crate) install: Installed,
    /// Present without waiting for the display (`--uncapped`).
    pub(crate) uncapped: bool,
    pub(crate) readout: Readout,
    /// Where the game's sounds are heard; `None` with `ROOK_AUDIO=off`.
    pub(crate) audio: Option<rook_audio::Output<rook_dom::InstanceId>>,
    /// The terminal command prompt, when the terminal is interactive.
    #[cfg(feature = "repl")]
    pub(crate) repl: Option<rook_engine::console::repl::Repl>,
}

impl App {
    pub(crate) fn init_gfx(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title(&self.title)
                    .with_inner_size(winit::dpi::LogicalSize::new(1280, 720)),
            )?,
        );
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
        config.present_mode = if self.uncapped {
            wgpu::PresentMode::AutoNoVsync
        } else {
            wgpu::PresentMode::AutoVsync
        };
        surface.configure(&device, &config);
        let mut renderer = Renderer::new(&adapter, device, queue, config.format);
        self.install.apply(&mut renderer);
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

    pub(crate) fn update(&mut self, dt: f32) {
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
        let pointing = self.game.hovering_click_detector();
        if pointing != self.pointing
            && let Some(gfx) = &self.gfx
        {
            gfx.window.set_cursor(if pointing {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            });
            self.pointing = pointing;
        }

        #[cfg(feature = "repl")]
        if let Some(repl) = &mut self.repl {
            let game = &self.game;
            repl.poll(game.has_client(), |side, line| game.repl(side, line));
        }

        // The client steps at 60 Hz, catching up at most a few per frame;
        // the server keeps its own pace on its thread.
        self.accumulator += f64::from(dt);
        self.clock += f64::from(dt);
        let mut steps = 0;
        while self.accumulator >= STEP && steps < 4 {
            self.accumulator -= STEP;
            steps += 1;
        }
        if steps == 4 {
            self.accumulator = 0.0;
        }
        self.game.advance(steps);
        if let Some(audio) = &mut self.audio {
            audio.sync(&self.game.sounds());
        }
        if let Some(gfx) = &mut self.gfx {
            let camera = self.camera.camera.position;
            refresh(
                &mut self.scene,
                self.game.view_mut(),
                &mut gfx.renderer,
                camera,
            );
        }
    }

    pub(crate) fn draw(&mut self) {
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
        // Wrapped so the shader's sines stay precise in long sessions.
        gfx.renderer.set_time((self.clock % 3600.0) as f32);
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

impl App {
    /// Start or stop looking around with the right mouse button: the
    /// cursor hides and stays where it was, as in Studio, instead of
    /// wandering out of the window while the camera turns.
    pub(crate) fn look(&mut self, on: bool) {
        self.looking = on;
        self.look_anchor = None;
        let Some(gfx) = &self.gfx else {
            return;
        };
        let window = &gfx.window;
        window.set_cursor_visible(!on);
        if !on {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            return;
        }
        // Locked holds the cursor still where the platform can; otherwise
        // confine it and warp it back after each move.
        if window.set_cursor_grab(CursorGrabMode::Locked).is_err() {
            let _ = window.set_cursor_grab(CursorGrabMode::Confined);
            self.look_anchor = Some(self.cursor);
        }
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
                // While looking around the cursor is held in place; its
                // moves (and our own warps back) aren't mouse moves.
                if !self.looking {
                    self.cursor = position;
                    self.pending_mouse = Some((position.x as f32, position.y as f32));
                }
            }
            WindowEvent::Focused(false) if self.looking => self.look(false),
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
                    self.look(down);
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
            // Where the cursor can only be confined to the window (Windows),
            // put it back after each move.
            if let (Some(anchor), Some(gfx)) = (self.look_anchor, &self.gfx) {
                let _ = gfx.window.set_cursor_position(anchor);
            }
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(gfx) = &self.gfx {
            gfx.window.request_redraw();
        }
    }
}
