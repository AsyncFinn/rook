//! The renderer: GPU objects, pipelines and what the scene needs on the GPU.

mod draw;
mod images;
mod pipelines;
mod placement;
mod readback;

use super::*;
pub(crate) use images::ImageSet;
use images::{Defaults, image_set};
use pipelines::Pipelines;
use placement::Placement;

/// GPU objects shared by windowed and offscreen rendering.
#[derive(Debug)]
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    color_format: wgpu::TextureFormat,
    pipelines: Pipelines,
    globals: wgpu::Buffer,
    pub(crate) bind_group: wgpu::BindGroup,
    /// The shadow map, drawn by the shadow pass and sampled by the main
    /// pass through `main_bind_group`, with the surface patterns.
    shadow_view: wgpu::TextureView,
    shadow_sampler: wgpu::Sampler,
    main_layout: wgpu::BindGroupLayout,
    main_bind_group: wgpu::BindGroup,
    image_layout: wgpu::BindGroupLayout,
    image_sampler: wgpu::Sampler,
    /// What stands in for images a draw doesn't have.
    defaults: Defaults,
    /// Each image set's bind group (group 2).
    groups: HashMap<ImageSet, wgpu::BindGroup>,
    /// The built-in shapes and each mesh file a part has drawn, by shape.
    meshes: GpuCache<DrawShape, Mesh>,
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
    terrain: TerrainLook,
    /// Seconds on the clock water and grass move by ([`Renderer::set_time`]).
    time: f32,
    depth: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
}

impl Renderer {
    /// Create a renderer drawing into `color_format` targets.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        color_format: wgpu::TextureFormat,
    ) -> Self {
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
            ],
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook images"),
            entries: &(0..5)
                .map(|i| texture_entry(i, wgpu::TextureViewDimension::D2))
                .collect::<Vec<_>>(),
        });
        let pipelines = Pipelines::new(&device, color_format, &layout, &main_layout, &image_layout);

        let cx = UploadContext {
            device: &device,
            queue: &queue,
            image_layout: &image_layout,
            sampler: &image_sampler,
        };
        let defaults = Defaults::new(&cx);
        let main_bind_group = surface_bind_group(
            &cx,
            &main_layout,
            &shadow_view,
            &shadow_sampler,
            &surfaces::rooks_own(),
        );
        let mut meshes = GpuCache::new();
        for shape in DrawShape::ALL {
            meshes.insert(shape, GpuMesh::upload(&device, &mesh::build(shape)));
        }

        let mut renderer = Self {
            device,
            queue,
            color_format,
            pipelines,
            globals,
            bind_group,
            shadow_view,
            shadow_sampler,
            main_layout,
            main_bind_group,
            image_layout,
            image_sampler,
            defaults,
            groups: HashMap::new(),
            meshes,
            images: GpuCache::new(),
            opaque: Batches::default(),
            layers: Batches::default(),
            placements: Vec::new(),
            waiting: HashMap::new(),
            transparent_free: Vec::new(),
            transparent_buffer: None,
            transparent_parts: Vec::new(),
            lighting: Lighting::default(),
            terrain: TerrainLook::default(),
            time: 0.0,
            depth: None,
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
        }
    }

    /// Free a mesh file's GPU copy once no entry draws it (the player's
    /// grass tiles, dropped as the camera leaves them). An entry still
    /// drawing it draws nothing until the scene hands the mesh over again.
    pub fn drop_mesh(&mut self, mesh: MeshHash) {
        self.meshes.remove(&DrawShape::File(mesh));
    }

    /// Set the clock water's waves and grass's sway move by, in seconds.
    pub fn set_time(&mut self, seconds: f32) {
        self.time = seconds;
    }

    /// Draw the Head MeshType with this mesh (the user's own `head.mesh`)
    /// instead of Rook's. It must be the head at its own size, as Roblox's
    /// file is.
    pub fn set_head_mesh(&mut self, head: &Mesh) {
        let gpu = GpuMesh::upload(&self.device, &mesh::from_file(head));
        self.meshes.insert(DrawShape::Head, gpu);
    }

    /// Draw the surface patterns from this atlas (the user's own
    /// `textures/studs.dds`) instead of Rook's. Returns false, keeping
    /// Rook's, when it isn't laid out as that file is.
    pub fn set_surface_atlas(&mut self, atlas: &Image) -> bool {
        let Some(patterns) = surfaces::from_studs_atlas(atlas) else {
            return false;
        };
        self.main_bind_group = surface_bind_group(
            &self.upload_context(),
            &self.main_layout,
            &self.shadow_view,
            &self.shadow_sampler,
            &patterns,
        );
        true
    }

    /// A renderer without a window, for screenshots and tests.
    pub fn headless() -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| RenderError::Adapter(e.to_string()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rook"),
            ..Default::default()
        }))
        .map_err(|e| RenderError::Device(e.to_string()))?;
        Ok(Self::new(
            device,
            queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ))
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }
}
