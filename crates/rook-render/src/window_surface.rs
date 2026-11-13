//! The renderer owns a window's surface as it owns its device, so apps
//! never touch the GPU API.

use std::time::{Duration, Instant};

use crate::{Camera, RenderError, Renderer};

#[derive(Debug)]
pub struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    presented: bool,
}

fn present_mode(vsync: bool) -> wgpu::PresentMode {
    if vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
    }
}

impl WindowSurface {
    pub fn new(
        window: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
        vsync: bool,
    ) -> Result<Self, RenderError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance
            .create_surface(window)
            .map_err(|e| RenderError::Surface(e.to_string()))?;
        let adapter = crate::renderer::adapter(&instance, Some(&surface))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Renderer::device_descriptor(&adapter)))
                .map_err(|e| RenderError::Device(e.to_string()))?;
        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or_else(|| RenderError::Surface("not supported by the adapter".into()))?;
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
        let mut renderer = Renderer::new(&adapter, device, queue, config.format);
        renderer.size_targets(config.width, config.height);
        Ok(Self {
            surface,
            config,
            renderer,
            presented: false,
        })
    }

    pub fn renderer(&self) -> &Renderer {
        &self.renderer
    }

    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn presented(&self) -> bool {
        self.presented
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        self.config.present_mode = present_mode(vsync);
        self.surface.configure(&self.renderer.device, &self.config);
    }

    /// False for a size there's nothing to draw into (a minimized window).
    pub fn resize(&mut self, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 {
            return false;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.renderer.device, &self.config);
        self.renderer.size_targets(width, height);
        true
    }

    /// `None` when the surface had to be set up again.
    fn next_frame(&self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            _ => {
                self.surface.configure(&self.renderer.device, &self.config);
                None
            }
        }
    }

    /// How long it waited for the display to free an image, which isn't
    /// the frame's cost.
    pub fn draw(&mut self, camera: &Camera, clock: f64, pre_present: impl FnOnce()) -> Duration {
        self.presented = false;
        let asked = Instant::now();
        let Some(frame) = self.next_frame() else {
            self.renderer.submit_undrawn();
            return asked.elapsed();
        };
        let display = asked.elapsed();
        let view = frame.texture.create_view(&Default::default());
        // Wrapped so the shader's sines stay precise in long sessions.
        self.renderer.set_time((clock % 3600.0) as f32);
        let (width, height) = (self.config.width, self.config.height);
        self.renderer.render(&view, width, height, camera);
        pre_present();
        self.renderer.queue.present(frame);
        self.presented = true;
        display
    }
}
