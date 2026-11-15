//! GUI quads on the GPU: the instance each quad draws as, and the buffers
//! that hold a list of them, written where they changed.

use super::*;

/// A list of quads as the shader reads them, each with its image.
pub(super) type Packed = Vec<(GpuQuad, Option<ImageKey>)>;

/// A screen piece or world canvas given anew, packed and uploaded.
pub(crate) enum Fresh {
    Piece(Arc<[GuiQuad]>),
    Canvas(Arc<WorldCanvas>),
}

impl Fresh {
    pub(crate) fn quads(&self) -> &[GuiQuad] {
        match self {
            Fresh::Piece(p) => p,
            Fresh::Canvas(c) => &c.quads,
        }
    }
}

/// What a list of quads shows that's drawn before it: its groups, inner
/// ones first, and the ViewportFrames in it or its groups.
#[derive(Debug, Default)]
pub(in crate::renderer) struct Held {
    pub(super) groups: Vec<Arc<GuiGroup>>,
    pub(super) viewports: Vec<GuiViewport>,
}

impl Held {
    pub(super) fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.viewports.is_empty()
    }
}

/// The quads' vertex attributes, in [`GpuQuad`]'s order.
pub(super) const QUAD_ATTRIBUTES: [wgpu::VertexAttribute; 11] = wgpu::vertex_attr_array![
    0 => Float32x2, 1 => Float32x2, 2 => Float32x2,
    3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32,
    7 => Float32x4, 8 => Float32x4, 9 => Float32x4, 10 => Float32x4,
];

/// A [`GuiQuad`] as the shader reads it.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(in crate::renderer) struct GpuQuad {
    pub(super) center: [f32; 2],
    pub(super) half_size: [f32; 2],
    pub(super) turn: [f32; 2],
    pub(super) color: [f32; 4],
    pub(super) clip: [f32; 4],
    /// The glyph's rectangle in the atlas, or the image's part, 0 to 1.
    pub(super) uv: [f32; 4],
    /// 0 fills the rectangle, 1 draws through a coverage mask, 2 through a
    /// distance field, 3 through an image, 4 through a group's texture, 5
    /// through a ViewportFrame's picture, 6 and up outlines text
    /// (`shaders/gui/gui_paint.wgsl`).
    pub(super) mode: f32,
    /// The shape's center and half size (0: none).
    pub(super) shape_rect: [f32; 4],
    /// The shape's corner radii, as [`rook_types::GuiShape`]'s.
    pub(super) radii: [f32; 4],
    /// The shape's band and cut corners (1), and the gradient's ramp row
    /// (-1: none).
    pub(super) shape: [f32; 4],
    /// Where the gradient is 0.5, and its change per pixel.
    pub(super) gradient: [f32; 4],
}

impl GpuQuad {
    /// `q`, its glyph or image at `uv`, its gradient's ramp on row `ramp`.
    pub(super) fn new(q: &GuiQuad, uv: [f32; 4], ramp: Option<u32>) -> Self {
        let a = q.color[3].clamp(0.0, 1.0);
        let (sin, cos) = q.rotation.sin_cos();
        let (shape_rect, radii, [ring, bevel]) = match &q.shape {
            Some(s) => (
                [s.center.x, s.center.y, s.size.x * 0.5, s.size.y * 0.5],
                s.radii,
                [s.ring, if s.bevel { 1.0 } else { 0.0 }],
            ),
            None => ([0.0; 4], [0.0; 4], [0.0; 2]),
        };
        let gradient = (q.gradient.as_ref())
            .map_or([0.0; 4], |g| [g.origin.x, g.origin.y, g.step.x, g.step.y]);
        Self {
            center: q.center.to_array(),
            half_size: (q.size * 0.5).to_array(),
            turn: [sin, cos],
            color: [q.color[0] * a, q.color[1] * a, q.color[2] * a, a],
            clip: q.clip,
            uv,
            mode: match &q.paint {
                Paint::Solid => 0.0,
                Paint::Glyph(mask) if mask.distance => 2.0,
                Paint::Glyph(_) => 1.0,
                Paint::Image(_) => 3.0,
                Paint::Group(_) => 4.0,
                Paint::Viewport(_) => 5.0,
                Paint::Outline(_, grow) => 6.0 + grow,
            },
            shape_rect,
            radii,
            shape: [ring, bevel, ramp.map_or(-1.0, |r| r as f32), 0.0],
            gradient,
        }
    }

    /// The quad as drawn `k` times bigger with its origin at `corner`
    /// (a canvas's quads laid on the screen).
    pub(super) fn moved(self, corner: glam::Vec2, k: f32) -> Self {
        let at = |x: f32, y: f32| [corner.x + x * k, corner.y + y * k];
        let [l, t] = at(self.clip[0], self.clip[1]);
        let [r, b] = at(self.clip[2], self.clip[3]);
        let [sx, sy] = at(self.shape_rect[0], self.shape_rect[1]);
        let [gx, gy] = at(self.gradient[0], self.gradient[1]);
        Self {
            center: at(self.center[0], self.center[1]),
            half_size: self.half_size.map(|h| h * k),
            clip: [l, t, r, b],
            shape_rect: [sx, sy, self.shape_rect[2] * k, self.shape_rect[3] * k],
            radii: self.radii.map(|r| r * k),
            gradient: [gx, gy, self.gradient[2] / k, self.gradient[3] / k],
            ..self
        }
    }

    /// The quads as instances, one per quad.
    pub(super) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuQuad>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &QUAD_ATTRIBUTES,
        }
    }
}

/// What the GUI pass draws into a target: the screen's GUI layer or a
/// group's texture.
#[derive(Debug)]
pub(super) struct Target {
    pub(super) size: wgpu::Buffer,
    pub(super) group: wgpu::BindGroup,
    /// The quads, and how many there are.
    pub(super) quads: Option<(wgpu::Buffer, u32)>,
    pub(super) runs: Vec<Run>,
}

impl Target {
    pub(super) fn new(device: &wgpu::Device, binds: Binds<'_>) -> Self {
        let size = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rook gui target size"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook gui"),
            layout: binds.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: size.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(binds.glyphs),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(binds.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(binds.ramps),
                },
            ],
        });
        Self {
            size,
            group,
            quads: None,
            runs: Vec::new(),
        }
    }

    pub(super) fn set(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, packed: &Packed) {
        self.runs = runs(packed.iter().map(|(_, image)| *image));
        if packed.is_empty() {
            self.quads = None;
            return;
        }
        let gpu: Vec<GpuQuad> = packed.iter().map(|(q, _)| *q).collect();
        let bytes: &[u8] = bytemuck::cast_slice(&gpu);
        let reuse = matches!(&self.quads, Some((b, _)) if b.size() >= bytes.len() as u64);
        if !reuse {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rook gui quads"),
                size: (bytes.len() as u64).next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.quads = Some((buffer, 0));
        }
        if let Some((buffer, count)) = &mut self.quads {
            queue.write_buffer(buffer, 0, bytes);
            *count = gpu.len() as u32;
        }
    }
}

impl Target {
    /// Write `packed` over the quads from the `at`th on, which must be
    /// in the buffer already.
    pub(super) fn write(&self, queue: &wgpu::Queue, at: u32, packed: &Packed) {
        let Some((buffer, _)) = &self.quads else {
            return;
        };
        if packed.is_empty() {
            return;
        }
        let gpu: Vec<GpuQuad> = packed.iter().map(|(q, _)| *q).collect();
        let offset = u64::from(at) * std::mem::size_of::<GpuQuad>() as u64;
        queue.write_buffer(buffer, offset, bytemuck::cast_slice(&gpu));
    }
}

/// What a [`Target`]'s bind group binds.
#[derive(Clone, Copy)]
pub(super) struct Binds<'a> {
    pub(super) layout: &'a wgpu::BindGroupLayout,
    pub(super) glyphs: &'a wgpu::TextureView,
    pub(super) ramps: &'a wgpu::TextureView,
    pub(super) sampler: &'a wgpu::Sampler,
}
