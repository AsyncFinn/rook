//! The GUI pass: the GUI's rectangles drawn in order into the GUI layer,
//! premultiplied and in sRGB, which the tone pass lays over the frame, so
//! GUI blends in sRGB as Roblox's does (docs/behavior/gui.md#drawing).
//! Text quads draw through their glyph's coverage in the atlas.

mod atlas;

use wesl_core::include_wesl;

use super::*;
use atlas::Atlas;

/// The GUI layer's format: 8-bit sRGB values, blended as stored.
pub(crate) const GUI_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A [`GuiQuad`] as the shader reads it.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuQuad {
    center: [f32; 2],
    half_size: [f32; 2],
    turn: [f32; 2],
    color: [f32; 4],
    clip: [f32; 4],
    /// The glyph's rectangle in the atlas, 0 to 1; a negative left fills.
    uv: [f32; 4],
}

impl GpuQuad {
    fn new(q: &GuiQuad, uv: [f32; 4]) -> Self {
        let a = q.color[3].clamp(0.0, 1.0);
        let (sin, cos) = q.rotation.sin_cos();
        Self {
            center: q.center.to_array(),
            half_size: (q.size * 0.5).to_array(),
            turn: [sin, cos],
            color: [q.color[0] * a, q.color[1] * a, q.color[2] * a, a],
            clip: q.clip,
            uv,
        }
    }
}

/// What a filled quad's `uv` holds.
const FILL: [f32; 4] = [-1.0; 4];

#[derive(Debug)]
pub(crate) struct GuiPass {
    pipeline: wgpu::RenderPipeline,
    /// The target's size.
    screen: wgpu::Buffer,
    group: wgpu::BindGroup,
    /// The quads, and how many there are.
    quads: Option<(wgpu::Buffer, u32)>,
    atlas: Atlas,
}

impl GuiPass {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook gui"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let atlas = Atlas::new(device);
        let atlas_view = atlas.texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rook gui glyphs"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let screen = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rook gui screen"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook gui"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: screen.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook gui"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("gui").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rook gui"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rook gui"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_gui"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuQuad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, 1 => Float32x2, 2 => Float32x2,
                        3 => Float32x4, 4 => Float32x4, 5 => Float32x4,
                    ],
                })],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_gui"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: GUI_FORMAT,
                    blend: Some(wgpu::BlendState {
                        color: premultiplied,
                        alpha: premultiplied,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            screen,
            group,
            quads: None,
            atlas,
        }
    }

    /// Draw `quads` from now on, bottom first.
    pub(crate) fn set(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, quads: &[GuiQuad]) {
        let gpu = self.pack(queue, quads);
        if gpu.is_empty() {
            self.quads = None;
            return;
        }
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

    /// `quads` as the shader reads them, their glyphs in the atlas. If the
    /// atlas fills and starts again partway, every glyph is placed again
    /// once, so none points at a mask that was written over.
    fn pack(&mut self, queue: &wgpu::Queue, quads: &[GuiQuad]) -> Vec<GpuQuad> {
        let scale = 1.0 / atlas::SIZE as f32;
        let once = |atlas: &mut Atlas| -> Vec<GpuQuad> {
            quads
                .iter()
                .filter_map(|q| {
                    let uv = match &q.glyph {
                        None => FILL,
                        Some(mask) => atlas.place(queue, mask)?.map(|t| t * scale),
                    };
                    Some(GpuQuad::new(q, uv))
                })
                .collect()
        };
        let resets = self.atlas.resets;
        let gpu = once(&mut self.atlas);
        if self.atlas.resets == resets {
            return gpu;
        }
        once(&mut self.atlas)
    }

    /// Draw the GUI layer into `target`, if there's any GUI. Whether there
    /// was.
    pub(crate) fn record(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        (width, height): (u32, u32),
    ) -> bool {
        let Some((buffer, count)) = &self.quads else {
            return false;
        };
        let size = [width as f32, height as f32, 0.0, 0.0];
        queue.write_buffer(&self.screen, 0, bytemuck::bytes_of(&size));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rook gui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..6, 0..*count);
        true
    }
}
