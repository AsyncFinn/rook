//! Making the GUI pass: its pipelines, atlas, ramps and buffers, and the
//! world GUI's pipelines again when the parts' image layout changes.

use super::*;

impl GuiPass {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layouts: BindLayouts<'_>,
        table: bool,
    ) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
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
                texture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(3),
            ],
        });
        let atlas = Atlas::new(device);
        let glyphs = atlas.texture.create_view(&Default::default());
        let ramps = Ramps::new(device);
        let ramps_view = ramps.texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rook gui glyphs"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook gui"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("gui").into()),
        });
        let images = GuiImages::new(device, queue);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rook gui"),
            bind_group_layouts: &[Some(&layout), Some(&images.layout)],
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
                buffers: &[Some(GpuQuad::layout())],
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
        let binds = Binds {
            layout: &layout,
            glyphs: &glyphs,
            ramps: &ramps_view,
            sampler: &sampler,
        };
        let screen = Target::new(device, binds);
        let overlay = Target::new(device, binds);
        let cursor = Target::new(device, binds);
        let world = WorldGui::new(device, layouts, table, (&glyphs, &ramps_view));
        Self {
            pipeline,
            screen,
            overlay,
            cursor,
            screen_pieces: ScreenPieces::default(),
            groups: Groups::default(),
            viewports: Vec::new(),
            viewports_version: 0,
            world,
            layout,
            glyphs,
            ramps_view,
            sampler,
            atlas,
            ramps,
            images,
        }
    }

    /// Make the world GUI's pipelines again for the parts' new image
    /// layout.
    pub(crate) fn rebuild_world(&mut self, device: &wgpu::Device, layouts: BindLayouts<'_>) {
        self.world.rebuild(device, layouts);
    }
}
