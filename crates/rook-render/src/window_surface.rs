//! Drawing into a window: its surface, how frames are paced on it, and
//! presenting. The renderer owns this, as it owns its device, so an app
//! hands over a window (anything with a raw window handle: winit's on the
//! desktop or Android) and never touches the GPU API itself.

use std::time::{Duration, Instant};

use crate::{Camera, RenderError, Renderer};

/// A window's surface and the renderer that draws into it.
#[derive(Debug)]
pub struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    /// The last frame was presented.
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
    /// Open a GPU device for `window`, `width` by `height` pixels,
    /// presenting at the display's rate with `vsync`.
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
        let renderer = Renderer::new(&adapter, device, queue, config.format);
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

    /// The surface's size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Whether the last frame was presented.
    pub fn presented(&self) -> bool {
        self.presented
    }

    /// Present at the display's rate, or as soon as each frame is drawn.
    pub fn set_vsync(&mut self, vsync: bool) {
        self.config.present_mode = present_mode(vsync);
        self.surface.configure(&self.renderer.device, &self.config);
    }

    /// Take the window's new size; false for a size there's nothing to
    /// draw into (a minimized window).
    pub fn resize(&mut self, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 {
            return false;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.renderer.device, &self.config);
        true
    }

    /// The next frame to draw on, or `None` when the surface had to be
    /// set up again. Waits for the display at its rate.
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

    /// Draw a frame with `camera` at `clock` seconds and present it,
    /// calling `pre_present` just before (winit's `pre_present_notify`);
    /// how long it waited for the display to free an image to draw into.
    pub fn draw(&mut self, camera: &Camera, clock: f64, pre_present: impl FnOnce()) -> Duration {
        self.presented = false;
        let asked = Instant::now();
        let Some(frame) = self.next_frame() else {
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
