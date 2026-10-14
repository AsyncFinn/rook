//! wgpu renderer for Rook.
//!
//! The renderer draws a [`Scene`] snapshot: it never touches the DOM
//! (architecture §8). It draws primitive parts, the built-in mesh shapes
//! and mesh files with their colour, transparency, mesh textures, Plastic's
//! surface patterns and Neon, Decals and Textures over them, a sky colour,
//! a sun, ambient light, fog and the sun's shadows. Terrain comes later.

mod cache;
mod mesh;
mod scene;
mod shadow;
mod surfaces;

pub use scene::{
    Camera, DrawShape, Image, ImageHash, ImagePlacement, Layer, Lighting, MaterialLook, Mesh,
    MeshHash, NormalId, PartInstance, PartShape, SKYBOX_FACES, Scene, SurfacePattern, Tiling,
};

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

use cache::{FRAME_UPLOAD_BYTES, GpuCache, Upload, UploadContext};
use shadow::{SHADOW_DISTANCE, SHADOW_SIZE, ShadowView};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("no suitable GPU adapter: {0}")]
    Adapter(String),
    #[error("GPU device request failed: {0}")]
    Device(String),
    #[error("reading back the frame failed: {0}")]
    Readback(String),
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    sun_dir: [f32; 4],
    ambient: [f32; 4],
    outdoor_ambient: [f32; 4],
    fog_color: [f32; 4],
    fog_range: [f32; 4],
    shadow_view_proj: [[f32; 4]; 4],
    /// A texel in uv, the normal offset, the depth bias, and how far
    /// shadows reach (0 for none).
    shadow: [f32; 4],
    /// Each face's image right and up (`NormalId::image_axes`), in
    /// `NormalId` order; w holds the studs per surface pattern tile.
    faces: [[f32; 4]; 12],
    /// What reflective parts take on (`Lighting::reflection`, sRGB).
    sky: [f32; 4],
    /// From clip space to directions from the camera, for the skybox.
    sky_from_clip: [[f32; 4]; 4],
    /// Each skybox image's direction, right and up ([`SKYBOX_FACES`]).
    sky_faces: [[f32; 4]; 18],
}

/// Instance flags (see `shader.wgsl`).
const CAST: u32 = 1;
const NEON: u32 = 2;
const MODE_FACE: u32 = 1;
const MODE_TILED: u32 = 2;
const MODE_MESH_UV: u32 = 3;
const UPRIGHT: u32 = 1 << 7;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Instance {
    position: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    back: [f32; 3],
    size: [f32; 3],
    color: [f32; 4],
    /// The mesh's bounds in its own units: images on faces cover this box
    /// scaled by `size`.
    box_min: [f32; 3],
    box_max: [f32; 3],
    /// A Texture's studs per tile and offset.
    tile: [f32; 4],
    /// CAST, NEON, an image layer's mode (bits 2-3) and face (4-6),
    /// UPRIGHT (bit 7), and from bit 8 each face's surface pattern + 1,
    /// four bits per face (wgpu allows 16 vertex attributes in all).
    flags: u32,
    /// A part's `Reflectance`.
    reflectance: f32,
}

impl Instance {
    fn new(p: &PartInstance, (box_min, box_max): (Vec3, Vec3)) -> Self {
        let mut flags = 0;
        let mut tile = [1.0, 1.0, 0.0, 0.0];
        let mut reflectance = 0.0;
        if p.upright {
            flags |= UPRIGHT;
        }
        match p.layer {
            Layer::Base => {
                if p.cast_shadow {
                    flags |= CAST;
                }
                if p.material == MaterialLook::Neon {
                    flags |= NEON;
                }
                reflectance = p.reflectance.clamp(0.0, 1.0);
                if p.material == MaterialLook::Plastic {
                    for (face, pattern) in p.surfaces.iter().enumerate() {
                        if let Some(pattern) = pattern {
                            flags |= (*pattern as u32 + 1) << (8 + face * 4);
                        }
                    }
                }
            }
            Layer::Image { placement, .. } => {
                let (mode, face) = match placement {
                    ImagePlacement::Face(face) => (MODE_FACE, face.index() as u32),
                    ImagePlacement::Tiled(face, t) => {
                        tile = [
                            t.studs_per_tile.x,
                            t.studs_per_tile.y,
                            t.offset.x,
                            t.offset.y,
                        ];
                        (MODE_TILED, face.index() as u32)
                    }
                    ImagePlacement::MeshUv => (MODE_MESH_UV, 0),
                };
                flags |= mode << 2 | face << 4;
            }
        }
        Self {
            position: p.position.to_array(),
            right: p.rotation.x_axis.to_array(),
            up: p.rotation.y_axis.to_array(),
            back: p.rotation.z_axis.to_array(),
            size: p.size.to_array(),
            color: [p.color[0], p.color[1], p.color[2], 1.0 - p.transparency],
            box_min: box_min.to_array(),
            box_max: box_max.to_array(),
            tile,
            flags,
            reflectance,
        }
    }
}

#[derive(Debug)]
struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    /// The mesh's bounds in its own units.
    bounds: (Vec3, Vec3),
}

impl GpuMesh {
    fn upload(device: &wgpu::Device, data: &mesh::MeshData) -> Self {
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rook mesh vertices"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rook mesh indices"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            vertices,
            indices,
            index_count: data.indices.len() as u32,
            bounds: data.bounds(),
        }
    }
}

impl Upload for Mesh {
    type Gpu = GpuMesh;

    fn upload_size(&self) -> usize {
        self.positions.len() * std::mem::size_of::<mesh::Vertex>() + self.indices.len() * 4
    }

    fn upload(&self, cx: &UploadContext<'_>) -> GpuMesh {
        profiling::scope!("upload mesh");
        GpuMesh::upload(cx.device, &mesh::from_file(self))
    }
}

/// An image on the GPU: its bind group (group 2) holds the texture.
#[derive(Debug)]
struct GpuImage {
    bind_group: wgpu::BindGroup,
}

impl Upload for Image {
    type Gpu = GpuImage;

    fn upload_size(&self) -> usize {
        self.byte_len()
    }

    fn upload(&self, cx: &UploadContext<'_>) -> GpuImage {
        profiling::scope!("upload image");
        let view = texture_with_mips(
            cx,
            "rook image",
            wgpu::TextureFormat::Rgba8UnormSrgb,
            self.width,
            self.height,
            &[self.mips.as_slice()],
        );
        GpuImage {
            bind_group: image_bind_group(cx, &view),
        }
    }
}

/// A texture of `layers`, each a mip chain (largest first) of RGBA8
/// `width` × `height`; one layer is a plain 2D texture.
fn texture_with_mips(
    cx: &UploadContext<'_>,
    label: &str,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    layers: &[&[Vec<u8>]],
) -> wgpu::TextureView {
    let mip_count = layers[0].len() as u32;
    let texture = cx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers.len() as u32,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, levels) in layers.iter().enumerate() {
        for (level, pixels) in levels.iter().enumerate() {
            let (w, h) = ((width >> level).max(1), (height >> level).max(1));
            cx.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    let dimension = if layers.len() > 1 {
        wgpu::TextureViewDimension::D2Array
    } else {
        wgpu::TextureViewDimension::D2
    };
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(dimension),
        ..Default::default()
    })
}

fn image_bind_group(cx: &UploadContext<'_>, view: &wgpu::TextureView) -> wgpu::BindGroup {
    cx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rook image"),
        layout: cx.image_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(view),
        }],
    })
}

/// What an instanced draw shares: its mesh and its image (a mesh texture,
/// or a layer's decal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BatchKey {
    shape: DrawShape,
    image: Option<ImageHash>,
}

/// Instances of one key, drawn instanced. An instance keeps its slot while
/// it draws this key; a slot it leaves holds a zeroed instance (which
/// draws nothing) until another takes it.
#[derive(Debug)]
struct Batch {
    key: BatchKey,
    instances: Vec<Instance>,
    free: Vec<usize>,
    /// The GPU copy and how many instances it has room for (made or grown
    /// when drawn).
    buffer: Option<(wgpu::Buffer, usize)>,
    /// Instances changed since they were last uploaded.
    dirty: Option<std::ops::Range<usize>>,
}

impl Batch {
    fn set(&mut self, slot: usize, instance: Instance) {
        self.instances[slot] = instance;
        self.dirty = Some(match self.dirty.take() {
            Some(r) => r.start.min(slot)..r.end.max(slot + 1),
            None => slot..slot + 1,
        });
    }
}

/// A set of batches, drawn in order of their `z` (`ZIndex` for image
/// layers, 0 for parts), then in the order they were made, so the draw
/// order is the same every run.
#[derive(Debug, Default)]
struct Batches {
    list: Vec<Batch>,
    of: HashMap<(i32, BatchKey), usize>,
    /// Indices into `list` in drawing order.
    order: Vec<usize>,
    /// Each batch's z.
    z: Vec<i32>,
}

impl Batches {
    fn clear(&mut self) {
        *self = Self::default();
    }

    /// Take a slot for `instance` in the batch of `(z, key)`.
    fn occupy(&mut self, z: i32, key: BatchKey, instance: Instance) -> (usize, usize) {
        let batch = match self.of.get(&(z, key)) {
            Some(&b) => b,
            None => {
                let b = self.list.len();
                self.list.push(Batch {
                    key,
                    instances: Vec::new(),
                    free: Vec::new(),
                    buffer: None,
                    dirty: None,
                });
                self.z.push(z);
                self.of.insert((z, key), b);
                let at = self.order.partition_point(|&o| self.z[o] <= z);
                self.order.insert(at, b);
                b
            }
        };
        let b = &mut self.list[batch];
        let slot = match b.free.pop() {
            Some(slot) => slot,
            None => {
                b.instances.push(Instance::zeroed());
                b.instances.len() - 1
            }
        };
        b.set(slot, instance);
        (batch, slot)
    }

    fn vacate(&mut self, batch: usize, slot: usize) {
        let b = &mut self.list[batch];
        b.set(slot, Instance::zeroed());
        b.free.push(slot);
    }

    /// Bring the batches' GPU copies up to date.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let size = std::mem::size_of::<Instance>();
        for b in &mut self.list {
            let len = b.instances.len();
            match &b.buffer {
                Some((buffer, capacity)) if *capacity >= len => {
                    if let Some(r) = b.dirty.take() {
                        queue.write_buffer(
                            buffer,
                            (r.start * size) as u64,
                            bytemuck::cast_slice(&b.instances[r]),
                        );
                    }
                }
                // New, or grown past its buffer: room to grow again.
                _ => {
                    let capacity = len + len / 2 + 16;
                    b.buffer = Some((instance_buffer(device, &b.instances, capacity), capacity));
                    b.dirty = None;
                }
            }
        }
    }

    /// (key, buffer, instances) in drawing order.
    fn draws(&self) -> Vec<(BatchKey, &wgpu::Buffer, std::ops::Range<u32>)> {
        self.order
            .iter()
            .filter_map(|&i| {
                let b = &self.list[i];
                let (buffer, _) = b.buffer.as_ref()?;
                Some((b.key, buffer, 0..b.instances.len() as u32))
            })
            .collect()
    }
}

/// A buffer holding `list` with room for `capacity` instances.
fn instance_buffer(device: &wgpu::Device, list: &[Instance], capacity: usize) -> wgpu::Buffer {
    let mut padded = list.to_vec();
    padded.resize(capacity.max(list.len()), Instance::zeroed());
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rook instances"),
        contents: bytemuck::cast_slice(&padded),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}

/// How a scene entry should be drawn now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Hidden,
    /// Its mesh or image is still being uploaded.
    Waiting,
    Opaque(BatchKey),
    Transparent,
    Layer(i32, BatchKey),
}

/// Where a scene entry is drawn, by its index in [`Scene::parts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// Invisible, a shape without a mesh, or waiting for an upload.
    Hidden,
    Opaque {
        batch: usize,
        slot: usize,
    },
    Layer {
        batch: usize,
        slot: usize,
    },
    /// An index into the transparent parts, which are sorted every frame.
    Transparent(usize),
}

/// Whether what an entry needs on the GPU is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Readiness {
    Ready,
    Waiting,
    Missing,
}

/// GPU objects shared by windowed and offscreen rendering.
#[derive(Debug)]
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    color_format: wgpu::TextureFormat,
    opaque_pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    /// Decals and Textures over what's drawn.
    layer_pipeline: wgpu::RenderPipeline,
    /// The skybox, behind everything.
    sky_pipeline: wgpu::RenderPipeline,
    /// Depth only, from the sun, over the opaque batches.
    shadow_pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The shadow map, drawn by the shadow pass and sampled by the main
    /// pass through `main_bind_group`, with the surface patterns.
    shadow_view: wgpu::TextureView,
    shadow_sampler: wgpu::Sampler,
    main_layout: wgpu::BindGroupLayout,
    main_bind_group: wgpu::BindGroup,
    image_layout: wgpu::BindGroupLayout,
    image_sampler: wgpu::Sampler,
    /// A 1 × 1 white image, for draws without one.
    white: GpuImage,
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
    depth: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
}

impl Renderer {
    /// Create a renderer drawing into `color_format` targets.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        color_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook parts"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
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
            label: Some("rook image"),
            entries: &[texture_entry(0, wgpu::TextureViewDimension::D2)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rook parts"),
            bind_group_layouts: &[Some(&layout), Some(&main_layout), Some(&image_layout)],
            immediate_size: 0,
        });
        // The shadow pass writes the map, so it can't bind it.
        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("rook shadow map"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let vertex_attrs = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x3, 4 => Uint32
        ];
        let instance_attrs = wgpu::vertex_attr_array![
            5 => Float32x3, 6 => Float32x3, 7 => Float32x3, 8 => Float32x3,
            9 => Float32x3, 10 => Float32x4, 11 => Float32x3, 12 => Float32x3,
            13 => Float32x4, 14 => Uint32, 15 => Float32
        ];
        let buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<mesh::Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &vertex_attrs,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Instance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &instance_attrs,
            }),
        ];
        let make = |label,
                    fragment,
                    blend: Option<wgpu::BlendState>,
                    depth_write: bool,
                    compare: wgpu::CompareFunction,
                    bias: wgpu::DepthBiasState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(compare),
                    stencil: Default::default(),
                    bias,
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rook shadow map"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Greater),
                stencil: Default::default(),
                // Pushes casters away from the sun (smaller in reverse Z)
                // where they slope across its view, against acne on faces
                // the sun grazes; the normal offset handles the rest.
                bias: wgpu::DepthBiasState {
                    constant: 0,
                    slope_scale: -1.5,
                    clamp: 0.0,
                },
            }),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        // Reverse Z: nearer is greater.
        let opaque_pipeline = make(
            "rook opaque",
            "fs_main",
            None,
            true,
            wgpu::CompareFunction::Greater,
            Default::default(),
        );
        let transparent_pipeline = make(
            "rook transparent",
            "fs_main",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            wgpu::CompareFunction::Greater,
            Default::default(),
        );
        // Images lie exactly on their part's surface: the same vertices
        // through the same (invariant) maths give the same depth.
        let layer_pipeline = make(
            "rook images",
            "fs_layer",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            wgpu::CompareFunction::GreaterEqual,
            // Towards the camera (greater in reverse Z), so an image also
            // wins over another part's face lying in the same plane, as in
            // Roblox, instead of fighting it.
            wgpu::DepthBiasState {
                constant: 4,
                slope_scale: 1.0,
                clamp: 0.0,
            },
        );

        // The skybox: one full-screen triangle per image, each keeping the
        // pixels whose direction its face covers; nothing written to depth,
        // so everything draws over it.
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rook sky"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_sky"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_sky"),
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

        let cx = UploadContext {
            device: &device,
            queue: &queue,
            image_layout: &image_layout,
            sampler: &image_sampler,
        };
        let white_view = texture_with_mips(
            &cx,
            "rook white",
            wgpu::TextureFormat::Rgba8UnormSrgb,
            1,
            1,
            &[&[vec![255; 4]]],
        );
        let white = GpuImage {
            bind_group: image_bind_group(&cx, &white_view),
        };
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

        Self {
            device,
            queue,
            color_format,
            opaque_pipeline,
            transparent_pipeline,
            layer_pipeline,
            sky_pipeline,
            shadow_pipeline,
            globals,
            bind_group,
            shadow_view,
            shadow_sampler,
            main_layout,
            main_bind_group,
            image_layout,
            image_sampler,
            white,
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
            depth: None,
        }
    }

    fn upload_context(&self) -> UploadContext<'_> {
        UploadContext {
            device: &self.device,
            queue: &self.queue,
            image_layout: &self.image_layout,
            sampler: &self.image_sampler,
        }
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

    /// Replace the scene. Opaque parts are grouped per (shape, image), each
    /// group drawn instanced, and uploaded once; image layers likewise;
    /// transparent parts are kept for per-frame sorting.
    pub fn set_scene(&mut self, scene: &Scene) {
        profiling::scope!("Renderer::set_scene");
        self.opaque.clear();
        self.layers.clear();
        self.placements.clear();
        self.waiting.clear();
        self.transparent_parts.clear();
        self.transparent_free.clear();
        self.update_parts(scene, &(0..scene.parts.len()).collect::<Vec<_>>());
    }

    /// Update the entries at `changed` (indices into `scene.parts`) and the
    /// lighting. Indices must stay with their entries from one call to the
    /// next; the scene may grow, and an entry that's no longer drawn should
    /// stay as a hidden one (fully transparent).
    pub fn update_parts(&mut self, scene: &Scene, changed: &[usize]) {
        profiling::scope!("Renderer::update_parts");
        self.lighting = scene.lighting;
        // The skybox's images go up like any other image.
        for hash in scene.lighting.skybox.into_iter().flatten() {
            self.image_readiness(hash, Some(scene));
        }
        if self.placements.len() < scene.parts.len() {
            self.placements.resize(scene.parts.len(), Placement::Hidden);
        }
        for &i in changed {
            let part = scene.parts[i];
            self.waiting.remove(&i);
            let target = self.target(&part, Some(scene));
            self.place(i, &part, target);
        }
    }

    /// Whether uploads are still queued (entries waiting on them draw
    /// nothing yet).
    pub fn is_uploading(&self) -> bool {
        self.meshes.has_queued() || self.images.has_queued()
    }

    /// Upload everything queued now, and place what waited on it
    /// (screenshots, which shouldn't miss what a few more frames would
    /// show).
    pub fn finish_uploads(&mut self) {
        self.upload_files(usize::MAX);
    }

    /// Upload queued files within `budget` bytes, then place the entries
    /// whose files are all there.
    fn upload_files(&mut self, mut budget: usize) {
        if !self.is_uploading() {
            return;
        }
        profiling::scope!("Renderer::upload_files");
        let cx = UploadContext {
            device: &self.device,
            queue: &self.queue,
            image_layout: &self.image_layout,
            sampler: &self.image_sampler,
        };
        let mut first = true;
        let meshes = self.meshes.upload(&cx, &mut budget, &mut first);
        let images = self.images.upload(&cx, &mut budget, &mut first);
        if !meshes && !images {
            return;
        }
        let waiting: Vec<(usize, PartInstance)> = self.waiting.drain().collect();
        for (i, part) in waiting {
            let target = self.target(&part, None);
            self.place(i, &part, target);
        }
    }

    /// Draw entry `i` as `target`, leaving where it was drawn before.
    fn place(&mut self, i: usize, part: &PartInstance, target: Target) {
        match (self.placements[i], target) {
            (Placement::Hidden, Target::Hidden) => {}
            (Placement::Transparent(t), Target::Transparent) => {
                self.transparent_parts[t] = Some(*part);
            }
            (Placement::Opaque { batch, slot }, Target::Opaque(key))
                if self.opaque.list[batch].key == key =>
            {
                let instance = Instance::new(part, self.bounds(part.shape));
                self.opaque.list[batch].set(slot, instance);
            }
            (Placement::Layer { batch, slot }, Target::Layer(z, key))
                if self.layers.list[batch].key == key && self.layers.z[batch] == z =>
            {
                let instance = Instance::new(part, self.bounds(part.shape));
                self.layers.list[batch].set(slot, instance);
            }
            (old, _) => {
                self.vacate(old);
                if target == Target::Waiting {
                    self.waiting.insert(i, *part);
                }
                self.placements[i] = self.occupy(part, target);
            }
        }
    }

    /// The bounds of a shape's mesh, in its own units.
    fn bounds(&self, shape: DrawShape) -> (Vec3, Vec3) {
        self.meshes
            .get(&shape)
            .map_or((Vec3::splat(-0.5), Vec3::splat(0.5)), |m| m.bounds)
    }

    /// How an entry should be drawn. With the scene, files it needs that
    /// aren't on the GPU are queued; without, they're only waited for.
    fn target(&mut self, part: &PartInstance, scene: Option<&Scene>) -> Target {
        let (image, z) = match part.layer {
            Layer::Base => (part.texture, None),
            Layer::Image {
                on_transparent: true,
                image,
                ..
            } => (Some(image), None),
            Layer::Image { image, z, .. } => (Some(image), Some(z)),
        };
        if part.transparency >= 1.0 {
            return Target::Hidden;
        }
        let mesh = self.mesh_readiness(part.shape, scene);
        let image_ready = image.map_or(Readiness::Ready, |hash| self.image_readiness(hash, scene));
        match (mesh, image_ready) {
            (Readiness::Missing, _) | (_, Readiness::Missing) => return Target::Hidden,
            (Readiness::Waiting, _) | (_, Readiness::Waiting) => return Target::Waiting,
            _ => {}
        }
        let key = BatchKey {
            shape: part.shape,
            image,
        };
        match z {
            Some(z) => Target::Layer(z, key),
            None if part.transparency > 0.0 || part.layer != Layer::Base => Target::Transparent,
            None => Target::Opaque(key),
        }
    }

    /// Whether a shape's mesh is on the GPU, queueing a mesh file the
    /// first time one is drawn. A file the scene doesn't hold, or one
    /// without triangles, draws nothing.
    fn mesh_readiness(&mut self, shape: DrawShape, scene: Option<&Scene>) -> Readiness {
        if self.meshes.contains(&shape) {
            return Readiness::Ready;
        }
        if self.meshes.is_queued(&shape) {
            return Readiness::Waiting;
        }
        let DrawShape::File(hash) = shape else {
            return Readiness::Missing;
        };
        match scene.and_then(|s| s.meshes.get(&hash)) {
            Some(file) if !file.indices.is_empty() => {
                self.meshes.request(shape, file);
                Readiness::Waiting
            }
            _ => Readiness::Missing,
        }
    }

    fn image_readiness(&mut self, hash: ImageHash, scene: Option<&Scene>) -> Readiness {
        if self.images.contains(&hash) {
            return Readiness::Ready;
        }
        if self.images.is_queued(&hash) {
            return Readiness::Waiting;
        }
        match scene.and_then(|s| s.images.get(&hash)) {
            Some(file) => {
                self.images.request(hash, file);
                Readiness::Waiting
            }
            None => Readiness::Missing,
        }
    }

    /// Free the slot an entry was drawn in.
    fn vacate(&mut self, placement: Placement) {
        match placement {
            Placement::Hidden => {}
            Placement::Opaque { batch, slot } => self.opaque.vacate(batch, slot),
            Placement::Layer { batch, slot } => self.layers.vacate(batch, slot),
            Placement::Transparent(t) => {
                self.transparent_parts[t] = None;
                self.transparent_free.push(t);
            }
        }
    }

    /// Take a slot to draw an entry in.
    fn occupy(&mut self, part: &PartInstance, target: Target) -> Placement {
        let instance = Instance::new(part, self.bounds(part.shape));
        match target {
            Target::Hidden | Target::Waiting => Placement::Hidden,
            Target::Transparent => {
                let t = match self.transparent_free.pop() {
                    Some(t) => t,
                    None => {
                        self.transparent_parts.push(None);
                        self.transparent_parts.len() - 1
                    }
                };
                self.transparent_parts[t] = Some(*part);
                Placement::Transparent(t)
            }
            Target::Opaque(key) => {
                let (batch, slot) = self.opaque.occupy(0, key, instance);
                Placement::Opaque { batch, slot }
            }
            Target::Layer(z, key) => {
                let (batch, slot) = self.layers.occupy(z, key, instance);
                Placement::Layer { batch, slot }
            }
        }
    }

    /// Upload the instances changed since the last frame, and the
    /// transparent ones sorted back to front for `camera` (an image on a
    /// transparent part just after its part). Returns the transparent
    /// draws: (key, whether they're images, instance range) runs in that
    /// order.
    fn upload_instances(&mut self, camera: &Camera) -> Vec<(BatchKey, bool, std::ops::Range<u32>)> {
        self.opaque.upload(&self.device, &self.queue);
        self.layers.upload(&self.device, &self.queue);
        // Back to front for blending.
        let mut order: Vec<&PartInstance> = self.transparent_parts.iter().flatten().collect();
        order.sort_by(|a, b| {
            let da = a.position.distance_squared(camera.position);
            let db = b.position.distance_squared(camera.position);
            let image = |p: &PartInstance| p.layer != Layer::Base;
            db.total_cmp(&da).then(image(a).cmp(&image(b)))
        });
        let instances: Vec<Instance> = order
            .iter()
            .map(|p| Instance::new(p, self.bounds(p.shape)))
            .collect();
        let mut runs: Vec<(BatchKey, bool, std::ops::Range<u32>)> = Vec::new();
        for (i, p) in order.iter().enumerate() {
            let i = i as u32;
            let (image, is_layer) = match p.layer {
                Layer::Base => (p.texture, false),
                Layer::Image { image, .. } => (Some(image), true),
            };
            let key = BatchKey {
                shape: p.shape,
                image,
            };
            match runs.last_mut() {
                Some((k, l, r)) if *k == key && *l == is_layer => r.end = i + 1,
                _ => runs.push((key, is_layer, i..i + 1)),
            }
        }
        if instances.is_empty() {
            return runs;
        }
        match &self.transparent_buffer {
            Some((buffer, capacity)) if *capacity >= instances.len() => {
                self.queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(&instances));
            }
            _ => {
                // Room to grow, so a few more parts don't make a new buffer.
                let capacity = instances.len() * 2;
                let buffer = instance_buffer(&self.device, &instances, capacity);
                self.transparent_buffer = Some((buffer, capacity));
            }
        }
        runs
    }

    fn depth_view(&mut self, width: u32, height: u32) -> wgpu::TextureView {
        if let Some((_, view, w, h)) = &self.depth
            && *w == width
            && *h == height
        {
            return view.clone();
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rook depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        self.depth = Some((texture, view.clone(), width, height));
        view
    }

    /// Draw the scene into `target` (a view of a `color_format` texture).
    /// Mesh files and images queued for upload go up first, within a
    /// per-frame budget.
    pub fn render(&mut self, target: &wgpu::TextureView, width: u32, height: u32, camera: &Camera) {
        profiling::scope!("Renderer::render");
        self.upload_files(FRAME_UPLOAD_BYTES);
        let l = self.lighting;
        let aspect = width as f32 / height.max(1) as f32;
        let sun = l.sun_direction.normalize_or(Vec3::Y);
        // No shadow pass at all with GlobalShadows off, or with the sun
        // below the horizon, where it lights nothing from above.
        let shadow =
            (l.global_shadows && sun.y > 0.0).then(|| ShadowView::fit(camera, aspect, sun));
        let rgb = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        let mut faces = [[0.0; 4]; 12];
        for face in NormalId::ALL {
            let (right, up) = face.image_axes();
            faces[face.index() * 2] = rgb(right.to_array(), rook_types::SURFACE_TILE_STUDS);
            faces[face.index() * 2 + 1] = rgb(up.to_array(), 0.0);
        }
        let globals = Globals {
            view_proj: camera.view_projection(aspect).to_cols_array_2d(),
            camera_pos: rgb(camera.position.to_array(), 1.0),
            sun_dir: rgb(sun.to_array(), 0.0),
            ambient: rgb(l.ambient, 0.0),
            outdoor_ambient: rgb(l.outdoor_ambient, 0.0),
            fog_color: rgb(l.fog_color, l.brightness),
            fog_range: [l.fog_start, l.fog_end, 0.0, 0.0],
            shadow_view_proj: shadow
                .map_or(glam::Mat4::IDENTITY, |s| s.view_proj)
                .to_cols_array_2d(),
            shadow: match shadow {
                Some(s) => [
                    1.0 / SHADOW_SIZE as f32,
                    s.normal_offset,
                    s.depth_bias,
                    SHADOW_DISTANCE,
                ],
                None => [0.0; 4],
            },
            faces,
            sky: rgb(l.reflection, 0.0),
            sky_from_clip: camera.sky_from_clip(aspect).to_cols_array_2d(),
            sky_faces: {
                let mut out = [[0.0; 4]; 18];
                for (i, (dir, right, up)) in SKYBOX_FACES.iter().enumerate() {
                    out[i * 3] = rgb(dir.to_array(), 0.0);
                    out[i * 3 + 1] = rgb(right.to_array(), 0.0);
                    out[i * 3 + 2] = rgb(up.to_array(), 0.0);
                }
                out
            },
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let transparent_runs = self.upload_instances(camera);

        let depth = self.depth_view(width, height);
        let sky = srgb_to_linear(l.sky_horizon);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rook frame"),
            });
        let opaque = self.opaque.draws();
        if shadow.is_some() {
            profiling::scope!("shadow pass");
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rook shadow map"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_pipeline(&self.shadow_pipeline);
            for (key, buffer, instances) in &opaque {
                self.draw(&mut pass, key.shape, None, buffer, instances.clone());
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rook parts"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(sky[0]),
                            g: f64::from(sky[1]),
                            b: f64::from(sky[2]),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.main_bind_group, &[]);
            // The skybox, once all six images are up; otherwise the clear
            // colour stands for it.
            let skybox = l.skybox.and_then(|faces| {
                let groups: Vec<_> = faces
                    .iter()
                    .map(|h| self.images.get(h).map(|g| &g.bind_group))
                    .collect::<Option<_>>()?;
                Some(groups)
            });
            if let Some(groups) = skybox {
                pass.set_pipeline(&self.sky_pipeline);
                for (face, group) in groups.into_iter().enumerate() {
                    pass.set_bind_group(2, group, &[]);
                    pass.draw(0..3, face as u32..face as u32 + 1);
                }
            }
            // Opaque parts, then images on them (before transparent parts,
            // so a window in front of a decal covers it), then transparent
            // parts back to front.
            for (pipeline, draws) in [
                (&self.opaque_pipeline, opaque),
                (&self.layer_pipeline, self.layers.draws()),
            ] {
                if draws.is_empty() {
                    continue;
                }
                pass.set_pipeline(pipeline);
                for (key, buffer, instances) in draws {
                    let image = Some(self.image_group(key.image));
                    self.draw(&mut pass, key.shape, image, buffer, instances);
                }
            }
            if let Some((buffer, _)) = &self.transparent_buffer {
                for (key, is_layer, instances) in transparent_runs {
                    pass.set_pipeline(if is_layer {
                        &self.layer_pipeline
                    } else {
                        &self.transparent_pipeline
                    });
                    let image = Some(self.image_group(key.image));
                    self.draw(&mut pass, key.shape, image, buffer, instances);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// The bind group of a draw's image (white when it has none).
    fn image_group(&self, image: Option<ImageHash>) -> &wgpu::BindGroup {
        image
            .and_then(|hash| self.images.get(&hash))
            .map_or(&self.white.bind_group, |g| &g.bind_group)
    }

    /// Draw `instances` of `buffer` as `shape`, with `image` bound as
    /// group 2 (not in the shadow pass).
    fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        shape: DrawShape,
        image: Option<&wgpu::BindGroup>,
        buffer: &wgpu::Buffer,
        instances: std::ops::Range<u32>,
    ) {
        // Entries are only drawn for shapes with a mesh.
        let Some(mesh) = self.meshes.get(&shape) else {
            return;
        };
        if let Some(group) = image {
            pass.set_bind_group(2, group, &[]);
        }
        pass.set_vertex_buffer(0, mesh.vertices.slice(..));
        pass.set_vertex_buffer(1, buffer.slice(..));
        pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.index_count, 0, instances);
    }

    /// Render offscreen and return tightly packed RGBA8 (sRGB) pixels.
    pub fn render_to_image(
        &mut self,
        width: u32,
        height: u32,
        camera: &Camera,
    ) -> Result<Vec<u8>, RenderError> {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rook screenshot"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.color_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        self.render(&view, width, height, camera);

        let row = (width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rook readback"),
            size: u64::from(row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        let data = slice
            .get_mapped_range()
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            let start = (y * row) as usize;
            out.extend_from_slice(&data[start..start + (width * 4) as usize]);
        }
        Ok(out)
    }
}

/// The main pass's group 1: the shadow map and the surface patterns.
fn surface_bind_group(
    cx: &UploadContext<'_>,
    layout: &wgpu::BindGroupLayout,
    shadow_view: &wgpu::TextureView,
    shadow_sampler: &wgpu::Sampler,
    patterns: &surfaces::Patterns,
) -> wgpu::BindGroup {
    let layers: Vec<&[Vec<u8>]> = patterns.layers.iter().map(Vec::as_slice).collect();
    // Linear: grey 0.5 is a factor of 1 after doubling.
    let atlas = texture_with_mips(
        cx,
        "rook surface patterns",
        wgpu::TextureFormat::Rgba8Unorm,
        patterns.size,
        patterns.size,
        &layers,
    );
    cx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rook shadow map and surfaces"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(shadow_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&atlas),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(cx.sampler),
            },
        ],
    })
}

fn srgb_to_linear(c: [f32; 3]) -> [f32; 3] {
    c.map(|x| {
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::Mat3;

    use super::*;

    fn part(shape: PartShape, x: f32, color: [f32; 3], transparency: f32) -> PartInstance {
        PartInstance {
            color,
            transparency,
            ..PartInstance::plain(
                DrawShape::Part(shape),
                Vec3::new(x, 0.0, 0.0),
                Mat3::IDENTITY,
                Vec3::splat(2.0),
            )
        }
    }

    /// A 2 × 2 image: red, green / blue, white.
    fn image() -> Arc<Image> {
        let pixels = [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255; 4],
        ];
        Arc::new(Image {
            width: 2,
            height: 2,
            format: rook_content::image::ImageFormat::Png,
            mips: rook_content::image::mips(pixels.concat(), 2, 2),
            translucent: false,
        })
    }

    /// Updating parts one by one draws what setting the whole scene draws:
    /// moves, recolours, a shape change, opaque to transparent and back,
    /// a part hidden, a new part in the slot it left, and decals and mesh
    /// textures coming and going.
    #[test]
    fn updating_parts_draws_what_a_new_scene_draws() {
        // Rendering needs a GPU adapter; machines without one skip this.
        let Ok(mut incremental) = Renderer::headless() else {
            return;
        };
        let Ok(mut fresh) = Renderer::headless() else {
            return;
        };
        let red = [1.0, 0.0, 0.0];
        let green = [0.0, 1.0, 0.0];
        let blue = [0.0, 0.0, 1.0];
        let hash = ImageHash([7; 32]);
        let decal = |x: f32| PartInstance {
            layer: Layer::Image {
                image: hash,
                placement: ImagePlacement::Face(NormalId::Back),
                z: 1,
                on_transparent: x > 0.0,
            },
            ..part(PartShape::Block, x, [1.0; 3], 0.0)
        };
        let mut scene = Scene {
            parts: vec![
                part(PartShape::Block, -6.0, red, 0.0),
                part(PartShape::Ball, -3.0, green, 0.0),
                part(PartShape::Block, 0.0, blue, 0.5),
                part(PartShape::Cylinder, 3.0, red, 0.0),
                part(PartShape::Block, 6.0, green, 0.0),
                decal(-6.0),
            ],
            images: HashMap::from([(hash, image())]),
            ..Scene::default()
        };
        scene.parts[3].material = MaterialLook::Plastic;
        scene.parts[3].surfaces = [Some(SurfacePattern::Studs); 6];
        incremental.set_scene(&scene);
        let camera = Camera::look_at(Vec3::new(0.0, 3.0, 16.0), Vec3::ZERO);
        incremental.render_to_image(64, 32, &camera).unwrap();

        scene.parts[0].position.y = 1.0;
        scene.parts[0].color = blue;
        scene.parts[1].shape = DrawShape::Part(PartShape::Block);
        scene.parts[1].texture = Some(hash);
        scene.parts[2].transparency = 0.0;
        scene.parts[3].transparency = 0.25;
        scene.parts[4].transparency = 1.0;
        scene.parts[5] = decal(3.0);
        scene.parts.push(part(PartShape::Wedge, 9.0, red, 0.0));
        incremental.update_parts(&scene, &[0, 1, 2, 3, 4, 5, 6]);
        // The hidden part's slot is taken by a new one.
        scene.parts[4] = part(PartShape::Ball, -9.0, blue, 0.0);
        incremental.update_parts(&scene, &[4]);
        incremental.finish_uploads();

        fresh.set_scene(&scene);
        fresh.finish_uploads();
        assert_eq!(
            incremental.render_to_image(64, 32, &camera).unwrap(),
            fresh.render_to_image(64, 32, &camera).unwrap()
        );
    }

    /// A decal shows its image on its face, and a part waiting for its
    /// image draws nothing until the upload.
    #[test]
    fn decals_draw_on_their_face() {
        let Ok(mut r) = Renderer::headless() else {
            return;
        };
        let hash = ImageHash([1; 32]);
        let mut base = part(PartShape::Block, 0.0, [0.5; 3], 0.0);
        base.size = Vec3::new(4.0, 4.0, 1.0);
        let decal = PartInstance {
            layer: Layer::Image {
                image: hash,
                placement: ImagePlacement::Face(NormalId::Back),
                z: 1,
                on_transparent: false,
            },
            color: [1.0; 3],
            ..base
        };
        let scene = Scene {
            parts: vec![base, decal],
            images: HashMap::from([(hash, image())]),
            lighting: Lighting {
                ambient: [1.0; 3],
                outdoor_ambient: [0.0; 3],
                brightness: 0.0,
                ..Lighting::default()
            },
            ..Scene::default()
        };
        r.set_scene(&scene);
        assert!(r.is_uploading());
        // From +Z the Back face's image is upright: red top left, white
        // bottom right.
        let camera = Camera {
            fov_y: 20.0,
            ..Camera::look_at(Vec3::new(0.0, 0.0, 20.0), Vec3::ZERO)
        };
        let (w, h) = (64, 64);
        let pixels = r.render_to_image(w, h, &camera).unwrap();
        assert!(!r.is_uploading());
        let at = |pixels: &[u8], x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [pixels[i], pixels[i + 1], pixels[i + 2]]
        };
        let pixels_after = r.render_to_image(w, h, &camera).unwrap();
        // The first frame uploaded the image, then drew it.
        assert_eq!(pixels, pixels_after);
        let top_left = at(&pixels_after, 24, 24);
        let bottom_right = at(&pixels_after, 40, 40);
        // Mostly red, mostly white (the 2 x 2 image is filtered).
        assert!(top_left[0] > top_left[1] + 100, "{top_left:?}");
        assert!(top_left[0] > top_left[2] + 100, "{top_left:?}");
        assert!(bottom_right.iter().all(|&c| c > 150), "{bottom_right:?}");
    }
}
