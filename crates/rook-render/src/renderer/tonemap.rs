//! The frame and the tone pass: the main pass draws light units into the
//! frame in Studio's encoding (`shaders/tone.wgsl`), blending there as
//! Studio blends, and the tone pass puts it through the place's tone curve
//! onto the target (docs/behavior/tone-mapping.md), Highlights and the
//! GUI layer over it.

use wesl_core::include_wesl;

use super::gui::GUI_FORMAT;
use super::highlights::{HIGHLIGHT_LIMIT, Looks, MASK_FORMAT};
use super::*;

/// What the tone pass reads besides the frame and the mask.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ToneUniform {
    /// x: the curve (0 Default, 1 Retro); y: 1 when the mask holds
    /// Highlights this frame; z: 1 when the GUI layer holds GUI.
    curve: [f32; 4],
    looks: Looks,
}

/// The frame's format. Studio's is 8 bits a channel; half floats keep the
/// same encoding without its banding.
pub(crate) const FRAME_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[derive(Debug)]
struct Frame {
    view: wgpu::TextureView,
    mask: wgpu::TextureView,
    gui: wgpu::TextureView,
    group: wgpu::BindGroup,
    size: (u32, u32),
}

#[derive(Debug)]
pub(crate) struct Tonemap {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    /// A [`ToneUniform`].
    uniform: wgpu::Buffer,
    /// The frame's and the Highlight mask's views, their size, and the
    /// bind group reading them.
    frame: Option<Frame>,
}

impl Tonemap {
    pub(crate) fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook tone"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook tone"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("tonemap").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rook tone"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rook tone"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_tonemap"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_tonemap"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rook tone"),
            size: std::mem::size_of::<ToneUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            uniform,
            frame: None,
        }
    }

    /// The frame, the Highlight mask and the GUI layer for a `width` ×
    /// `height` target, made again when the size changes.
    pub(crate) fn frame(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::TextureView, wgpu::TextureView, wgpu::TextureView) {
        if let Some(f) = &self.frame
            && f.size == (width, height)
        {
            return (f.view.clone(), f.mask.clone(), f.gui.clone());
        }
        let texture = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let view = texture("rook frame", FRAME_FORMAT);
        let mask = texture("rook highlight mask", MASK_FORMAT);
        let gui = texture("rook gui layer", GUI_FORMAT);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook tone"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&mask),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&gui),
                },
            ],
        });
        self.frame = Some(Frame {
            view: view.clone(),
            mask: mask.clone(),
            gui: gui.clone(),
            group,
            size: (width, height),
        });
        (view, mask, gui)
    }

    /// The frame through `tonemapper`'s curve onto `target`, with the
    /// Highlights' `looks` over it when the mask holds any, and the GUI
    /// layer over that when it holds GUI.
    pub(crate) fn record(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        tonemapper: Tonemapper,
        (looks, gui): (Option<&Looks>, bool),
    ) {
        let Some(Frame { group, .. }) = &self.frame else {
            return;
        };
        let curve = match tonemapper {
            Tonemapper::Default => 0.0f32,
            Tonemapper::Retro => 1.0,
        };
        let uniform = ToneUniform {
            curve: [
                curve,
                f32::from(u8::from(looks.is_some())),
                f32::from(u8::from(gui)),
                0.0,
            ],
            looks: looks.copied().unwrap_or([[0.0; 4]; 2 * HIGHLIGHT_LIMIT]),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rook tone"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}
