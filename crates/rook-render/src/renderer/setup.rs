//! Making a renderer: the device it asks for, with a window or without,
//! and the pipelines, buffers and bind groups it starts with.

use super::*;

pub(crate) fn adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, RenderError> {
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: surface,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .map_err(|e| RenderError::Adapter(e.to_string()))
}

impl Renderer {
    /// Create a renderer drawing into `color_format` targets, on a device
    /// made from `adapter`.
    pub fn new(
        adapter: &wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        color_format: wgpu::TextureFormat,
    ) -> Self {
        let view_formats = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::VIEW_FORMATS);
        let block_compression = device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rook globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook globals"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let (shadow_view, shadow_sampler) = shadow_map(&device);
        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rook images"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let main_layout = main_layout(&device);
        let image_binding = ImageBinding::for_device(&device);
        let table = image_binding.table_slots();
        let image_layout = image_layout(&device, table);
        let lights = GpuLights::new(&device);
        let pipelines = Pipelines::new(
            &device,
            FRAME_FORMAT,
            BindLayouts {
                globals: &layout,
                main: &main_layout,
                image: &image_layout,
                lights: &lights.layout,
            },
            table.is_some(),
        );

        let cx = UploadContext {
            device: &device,
            queue: &queue,
            image_layout: &image_layout,
            sampler: &image_sampler,
            view_formats,
            block_compression,
        };
        let defaults = Defaults::new(&cx);
        let own = surfaces::rooks_own();
        let surfaces_view = layers_view(&cx, "rook surface patterns", own.size, &own.layers);
        // Without the install's frames, a flat normal (X in alpha, Y in
        // green, as the install's DXT5 maps hold them).
        let water_view = layers_view(
            &cx,
            "rook water normals",
            1,
            &[vec![vec![128, 128, 255, 128]]],
        );
        let sky_cube = SkyCube::new(&device, &queue);
        let brdf_view = crate::lighting::sky::rooks_brdf_table(&device, &queue);
        let main_bind_group = main_bind_group(
            &cx,
            &main_layout,
            [&shadow_sampler, &sky_cube.image_sampler],
            [
                &shadow_view,
                &surfaces_view,
                &water_view,
                &sky_cube.view,
                &brdf_view,
            ],
        );
        let mut pool = MeshPool::new(&device, &queue);
        let mut meshes = GpuCache::new();
        for shape in DrawShape::ALL {
            meshes.insert(shape, pool.add(&mesh::build(shape)));
        }
        let multi_draw = crate::renderer::draws::runs::multi_draw(&device);

        let info = adapter.get_info();
        let gpu = format!(
            "{} ({:?}, driver {} {})",
            info.name, info.backend, info.driver, info.driver_info
        );
        let tonemap = Tonemap::new(&device, color_format);
        let scatter = crate::batch::scatter::Scatter::new(&device);
        let gui = GuiPass::new(
            &device,
            &queue,
            BindLayouts {
                globals: &layout,
                main: &main_layout,
                image: &image_layout,
                lights: &lights.layout,
            },
            table.is_some(),
        );
        let mut renderer = Self {
            gpu,
            device,
            queue,
            color_format,
            view_formats,
            block_compression,
            pipelines,
            globals,
            globals_layout: layout,
            bind_group,
            shadow_view,
            surfaces_view,
            water_view,
            sky_cube,
            brdf_view,
            shadow_sampler,
            main_layout,
            main_bind_group,
            image_layout,
            image_sampler,
            defaults,
            image_binding,
            meshes,
            pool,
            multi_draw,
            indirect: None,
            cuts: crate::renderer::passes::cuts::Cuts::default(),
            images: GpuCache::new(),
            world: World::default(),
            viewports: Viewports::default(),
            scatter,
            placements: Vec::new(),
            detail: crate::renderer::draws::detail::Detail::default(),
            waiting: crate::renderer::draws::waiting::Waiting::default(),
            highlights: HighlightParts::default(),
            lighting: Lighting::default(),
            lights,
            particles: GpuParticles::default(),
            ribbons: GpuRibbons::default(),
            terrain: TerrainLook::default(),
            time: 0.0,
            water_phase: 0.0,
            depth: None,
            tonemap,
            gui,
            draw_counts: DrawCounts::default(),
            timer: None,
            record_time: Default::default(),
            stage_times: StageTimes::default(),
        };
        renderer.ensure_group(lone_image(None));
        renderer
    }

    pub(super) fn rebuild_main_bind_group(&mut self) {
        self.main_bind_group = main_bind_group(
            &self.upload_context(),
            &self.main_layout,
            [&self.shadow_sampler, &self.sky_cube.image_sampler],
            [
                &self.shadow_view,
                &self.surfaces_view,
                &self.water_view,
                &self.sky_cube.view,
                &self.brdf_view,
            ],
        );
    }

    /// What to ask the adapter for when making the device a renderer
    /// gets: the features Rook uses where the adapter has them
    /// (timestamps for `--bench`, binding arrays for the image table,
    /// first instances in indirect draws for multi-draws, BC blocks for
    /// material textures), room for the
    /// table, a stencil with the depth for Glass (`main_depth_format`),
    /// and the adapter's biggest buffers (for the mesh pool).
    pub fn device_descriptor(adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
        let wanted = wgpu::Features::TIMESTAMP_QUERY
            | wgpu::Features::INDIRECT_FIRST_INSTANCE
            | wgpu::Features::TEXTURE_BINDING_ARRAY
            | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
            | wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY
            | wgpu::Features::DEPTH32FLOAT_STENCIL8
            | wgpu::Features::TEXTURE_COMPRESSION_BC;
        let required_features = adapter.features() & wanted;
        let mut required_limits = wgpu::Limits {
            max_buffer_size: adapter.limits().max_buffer_size,
            ..Default::default()
        };
        if required_features.contains(wgpu::Features::TEXTURE_BINDING_ARRAY) {
            let a = adapter.limits();
            required_limits.max_binding_array_elements_per_shader_stage =
                a.max_binding_array_elements_per_shader_stage;
            required_limits.max_sampled_textures_per_shader_stage =
                a.max_sampled_textures_per_shader_stage;
        }
        wgpu::DeviceDescriptor {
            label: Some("rook"),
            required_features,
            required_limits,
            ..Default::default()
        }
    }

    /// A renderer without a window, for screenshots and tests.
    pub fn headless() -> Result<Self, RenderError> {
        Self::headless_with(Self::device_descriptor)
    }

    /// A renderer without a window on a device `descriptor` asks for.
    pub(crate) fn headless_with(
        descriptor: impl FnOnce(&wgpu::Adapter) -> wgpu::DeviceDescriptor<'static>,
    ) -> Result<Self, RenderError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = adapter(&instance, None)?;
        let (device, queue) = pollster::block_on(adapter.request_device(&descriptor(&adapter)))
            .map_err(|e| RenderError::Device(e.to_string()))?;
        Ok(Self::new(
            &adapter,
            device,
            queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ))
    }
}
