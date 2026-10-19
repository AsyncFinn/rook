//! The renderer: GPU objects, pipelines and what the scene needs on the GPU.

pub(crate) mod shadow;

mod draw;
mod images;
mod lights;
mod particles;
mod pipelines;
mod placement;
mod readback;
mod runs;
mod timing;

use super::*;
pub(crate) use images::ImageBinding;
pub(crate) use images::ImageSet;
use images::{Defaults, image_layout, image_set};
use lights::GpuLights;
use particles::GpuParticles;
use pipelines::Pipelines;
use placement::Placement;
use timing::GpuTimer;
pub use timing::GpuTimes;

/// Water's phase wraps here: a whole number of its 25 normal frames'
/// cycles (20 units each) and of both waves' periods (40 and 25,
/// `shaders/terrain.wgsl`), so nothing jumps.
const WATER_PHASE_PERIOD: f64 = 200.0;

/// GPU objects shared by windowed and offscreen rendering.
#[derive(Debug)]
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// The GPU, its backend and driver, for logs and crash reports.
    gpu: String,
    color_format: wgpu::TextureFormat,
    /// See [`UploadContext::view_formats`].
    view_formats: bool,
    pipelines: Pipelines,
    globals: wgpu::Buffer,
    globals_layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    /// The shadow map, drawn by the shadow pass and sampled by the main
    /// pass through `main_bind_group`, with the surface patterns and
    /// water's normal frames.
    shadow_view: wgpu::TextureView,
    surfaces_view: wgpu::TextureView,
    water_view: wgpu::TextureView,
    shadow_sampler: wgpu::Sampler,
    main_layout: wgpu::BindGroupLayout,
    main_bind_group: wgpu::BindGroup,
    image_layout: wgpu::BindGroupLayout,
    image_sampler: wgpu::Sampler,
    /// What stands in for images a draw doesn't have.
    defaults: Defaults,
    /// The image table, or each image set's bind group (group 2).
    pub(crate) image_binding: ImageBinding,
    /// The built-in shapes and each mesh file a part has drawn, by shape,
    /// each a place in `pool`.
    meshes: GpuCache<DrawShape, Mesh>,
    pool: MeshPool,
    /// Whether each pipeline's draws go in multi-draws from `indirect`
    /// (the adapter takes a first instance in indirect draws), or one by
    /// one.
    pub(crate) multi_draw: bool,
    /// The frame's draws' arguments, and how many it has room for.
    indirect: Option<(wgpu::Buffer, usize)>,
    /// Each image something has drawn, by content hash.
    images: GpuCache<ImageHash, Image>,
    opaque: Batches,
    layers: Batches,
    /// How each entry of the scene is drawn, by its index in the scene.
    placements: Vec<Placement>,
    /// Entries waiting for an upload, placed once it's done.
    waiting: HashMap<usize, PartInstance>,
    /// The sorted transparent parts' instances, rewritten every frame, and
    /// how many it holds.
    transparent_buffer: Option<(wgpu::Buffer, usize)>,
    /// Slots for transparent parts (`None` when free), sorted every frame.
    transparent_parts: Vec<Option<PartInstance>>,
    transparent_free: Vec<usize>,
    lighting: Lighting,
    /// The local lights and their spatial hash (group 3).
    pub(crate) lights: GpuLights,
    /// The emitters' particles.
    particles: GpuParticles,
    terrain: TerrainLook,
    /// Seconds on the clock grass sways by ([`Renderer::set_time`]).
    time: f32,
    /// How far water has moved: `WaterWaveSpeed` × seconds, summed as the
    /// speed changes (observed: Studio's water stops where it is at speed
    /// 0 and goes on from there), wrapped at [`WATER_PHASE_PERIOD`].
    pub(crate) water_phase: f64,
    depth: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
    /// The last frame's draw calls.
    draw_counts: DrawCounts,
    /// The GPU's pass times, once asked for ([`Renderer::time_gpu`]).
    timer: Option<GpuTimer>,
    /// How long the last frame took the CPU to record and submit, file
    /// uploads included.
    record_time: std::time::Duration,
}

/// What a frame drew, in the shadow pass and the main pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DrawCounts {
    pub shadow: PassCounts,
    pub main: PassCounts,
}

/// What a pass drew: the draw calls recorded (a multi-draw is one), the
/// instanced draws in them, and their triangles (free instance slots
/// included).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PassCounts {
    pub calls: u32,
    pub draws: u32,
    pub triangles: u64,
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
        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rook shadow map"),
            size: wgpu::Extent3d {
                width: SHADOW_SIZE,
                height: SHADOW_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_texture.create_view(&Default::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rook shadow map"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            // Reverse Z: lit when the point is at least as near the sun as
            // what the map holds.
            compare: Some(wgpu::CompareFunction::GreaterEqual),
            ..Default::default()
        });
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
        let texture_entry = |binding, dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: dimension,
                multisampled: false,
            },
            count: None,
        };
        let main_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook shadow map and surfaces"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                texture_entry(2, wgpu::TextureViewDimension::D2Array),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture_entry(4, wgpu::TextureViewDimension::D2Array),
            ],
        });
        let image_binding = ImageBinding::for_device(&device);
        let table = image_binding.table_slots();
        let image_layout = image_layout(&device, table);
        let lights = GpuLights::new(&device);
        let pipelines = Pipelines::new(
            &device,
            color_format,
            &layout,
            &main_layout,
            &image_layout,
            &lights.layout,
            table.is_some(),
        );

        let cx = UploadContext {
            device: &device,
            queue: &queue,
            image_layout: &image_layout,
            sampler: &image_sampler,
            view_formats,
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
        let main_bind_group = main_bind_group(
            &cx,
            &main_layout,
            &shadow_view,
            &shadow_sampler,
            &surfaces_view,
            &water_view,
        );
        let mut pool = MeshPool::new(&device, &queue);
        let mut meshes = GpuCache::new();
        for shape in DrawShape::ALL {
            meshes.insert(shape, pool.add(&mesh::build(shape)));
        }
        let multi_draw = device
            .features()
            .contains(wgpu::Features::INDIRECT_FIRST_INSTANCE);

        let info = adapter.get_info();
        let gpu = format!(
            "{} ({:?}, driver {} {})",
            info.name, info.backend, info.driver, info.driver_info
        );
        let mut renderer = Self {
            gpu,
            device,
            queue,
            color_format,
            view_formats,
            pipelines,
            globals,
            globals_layout: layout,
            bind_group,
            shadow_view,
            surfaces_view,
            water_view,
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
            images: GpuCache::new(),
            opaque: Batches::default(),
            layers: Batches::default(),
            placements: Vec::new(),
            waiting: HashMap::default(),
            transparent_free: Vec::new(),
            transparent_buffer: None,
            transparent_parts: Vec::new(),
            lighting: Lighting::default(),
            lights,
            particles: GpuParticles::default(),
            terrain: TerrainLook::default(),
            time: 0.0,
            water_phase: 0.0,
            depth: None,
            draw_counts: DrawCounts::default(),
            timer: None,
            record_time: Default::default(),
        };
        renderer.ensure_group([None; 5]);
        renderer
    }

    pub(crate) fn upload_context(&self) -> UploadContext<'_> {
        UploadContext {
            device: &self.device,
            queue: &self.queue,
            image_layout: &self.image_layout,
            sampler: &self.image_sampler,
            view_formats: self.view_formats,
        }
    }

    /// Free a mesh file's GPU copy once no entry draws it (the player's
    /// grass tiles, dropped as the camera leaves them). An entry still
    /// drawing it draws nothing until the scene hands the mesh over again.
    pub fn drop_mesh(&mut self, mesh: MeshHash) {
        if let Some(gpu) = self.meshes.remove(&DrawShape::File(mesh)) {
            self.pool.remove(&gpu);
        }
    }

    /// Set the clock grass sways by, in seconds; water and particles move
    /// on by the time since the last call (none when the clock goes back,
    /// at most a second), water at the scene's `WaterWaveSpeed`.
    pub fn set_time(&mut self, seconds: f32) {
        let dt = (seconds - self.time).clamp(0.0, 1.0);
        self.particles.sim.step(dt);
        self.water_phase =
            (self.water_phase + f64::from(dt * self.terrain.wave_speed)) % WATER_PHASE_PERIOD;
        self.time = seconds;
    }

    /// Draw the Head MeshType with this mesh (the user's own `head.mesh`)
    /// instead of Rook's. It must be the head at its own size, as Roblox's
    /// file is.
    pub fn set_head_mesh(&mut self, head: &Mesh) {
        let gpu = self.pool.add(&mesh::from_file(head));
        if let Some(old) = self.meshes.insert(DrawShape::Head, gpu) {
            self.pool.remove(&old);
        }
    }

    /// Draw the surface patterns from this atlas (the user's own
    /// `textures/studs.dds`) instead of Rook's. Returns false, keeping
    /// Rook's, when it isn't laid out as that file is.
    pub fn set_surface_atlas(&mut self, atlas: &Image) -> bool {
        let Some(patterns) = surfaces::from_studs_atlas(atlas) else {
            return false;
        };
        self.surfaces_view = layers_view(
            &self.upload_context(),
            "rook surface patterns",
            patterns.size,
            &patterns.layers,
        );
        self.rebuild_main_bind_group();
        true
    }

    /// Ripple water with these normal maps (the install's
    /// `textures/water/normal_01.dds` to `normal_25.dds`, X in alpha and Y
    /// in green), an animation of one frame each, instead of a flat
    /// surface. Returns false, keeping the flat one, unless they're all
    /// square and the same size.
    pub fn set_water_normals(&mut self, frames: &[Image]) -> bool {
        let Some(first) = frames.first() else {
            return false;
        };
        let size = first.width;
        if frames.iter().any(|f| f.width != size || f.height != size) {
            return false;
        }
        let layers: Vec<Vec<Vec<u8>>> = frames.iter().map(|f| f.mips.clone()).collect();
        self.water_view = layers_view(&self.upload_context(), "rook water normals", size, &layers);
        self.rebuild_main_bind_group();
        true
    }

    fn rebuild_main_bind_group(&mut self) {
        self.main_bind_group = main_bind_group(
            &self.upload_context(),
            &self.main_layout,
            &self.shadow_view,
            &self.shadow_sampler,
            &self.surfaces_view,
            &self.water_view,
        );
    }

    /// What to ask the adapter for when making the device a renderer
    /// gets: the features Rook uses where the adapter has them
    /// (timestamps for `--bench`, binding arrays for the image table,
    /// first instances in indirect draws for multi-draws), room for the
    /// table, a stencil with the depth for Glass (`main_depth_format`),
    /// and the adapter's biggest buffers (for the mesh pool).
    pub fn device_descriptor(adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
        let wanted = wgpu::Features::TIMESTAMP_QUERY
            | wgpu::Features::INDIRECT_FIRST_INSTANCE
            | wgpu::Features::TEXTURE_BINDING_ARRAY
            | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
            | wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY
            | wgpu::Features::DEPTH32FLOAT_STENCIL8;
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
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| RenderError::Adapter(e.to_string()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&descriptor(&adapter)))
            .map_err(|e| RenderError::Device(e.to_string()))?;
        Ok(Self::new(
            &adapter,
            device,
            queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ))
    }

    /// How many draw calls the last frame took.
    pub fn draw_counts(&self) -> DrawCounts {
        self.draw_counts
    }

    /// How long the CPU took to record and submit the last frame, file
    /// uploads included (not the GPU's time: [`Renderer::gpu_times`]).
    pub fn record_time(&self) -> std::time::Duration {
        self.record_time
    }

    /// The GPU, its backend and driver.
    pub fn gpu(&self) -> &str {
        &self.gpu
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }
}
