//! wgpu renderer for Rook.
//!
//! The renderer draws a [`Scene`] snapshot: it never touches the DOM
//! (architecture §8). It draws primitive parts, the built-in mesh shapes
//! and mesh files with their colour and transparency, a sky colour, a sun,
//! ambient light and fog. Textures, decals, terrain and shadows come later.

mod mesh;
mod scene;

pub use scene::{Camera, DrawShape, Lighting, Mesh, MeshHash, PartInstance, PartShape, Scene};

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

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
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Instance {
    position: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    back: [f32; 3],
    size: [f32; 3],
    color: [f32; 4],
}

impl Instance {
    fn from_part(p: &PartInstance) -> Self {
        Self {
            position: p.position.to_array(),
            right: p.rotation.x_axis.to_array(),
            up: p.rotation.y_axis.to_array(),
            back: p.rotation.z_axis.to_array(),
            size: p.size.to_array(),
            color: [p.color[0], p.color[1], p.color[2], 1.0 - p.transparency],
        }
    }
}

#[derive(Debug)]
struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
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
        }
    }
}

/// Opaque parts of one shape, drawn instanced. A part keeps its slot while
/// it draws this shape; a slot it leaves holds a zeroed instance (which
/// draws nothing) until another part takes it.
#[derive(Debug)]
struct Batch {
    shape: DrawShape,
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

/// How a part should be drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Hidden,
    Opaque(DrawShape),
    Transparent,
}

/// Where a scene part is drawn, by its index in [`Scene::parts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// Invisible, or a shape without a mesh.
    Hidden,
    Opaque {
        batch: usize,
        slot: usize,
    },
    /// An index into the transparent parts, which are sorted every frame.
    Transparent(usize),
}

/// GPU objects shared by windowed and offscreen rendering.
#[derive(Debug)]
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    color_format: wgpu::TextureFormat,
    opaque: wgpu::RenderPipeline,
    transparent: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The built-in shapes, and each mesh file a part has drawn, uploaded
    /// once by content hash (kept for the renderer's life).
    meshes: HashMap<DrawShape, GpuMesh>,
    opaque_batches: Vec<Batch>,
    /// Each shape's batch.
    batch_of: HashMap<DrawShape, usize>,
    /// How each part of the scene is drawn, by its index in the scene.
    placements: Vec<Placement>,
    /// The sorted transparent parts' instances, rewritten every frame, and
    /// how many it holds.
    transparent_buffer: Option<(wgpu::Buffer, usize)>,
    /// Transparent parts, sorted back to front every frame.
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rook parts"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let vertex_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let instance_attrs = wgpu::vertex_attr_array![
            2 => Float32x3, 3 => Float32x3, 4 => Float32x3,
            5 => Float32x3, 6 => Float32x3, 7 => Float32x4
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
        let make = |label, blend: Option<wgpu::BlendState>, depth_write: bool| {
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
                    // Reverse Z: nearer is greater.
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
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
        let opaque = make("rook opaque", None, true);
        let transparent = make(
            "rook transparent",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
        );

        let meshes = DrawShape::ALL
            .iter()
            .map(|&shape| (shape, GpuMesh::upload(&device, &mesh::build(shape))))
            .collect();

        Self {
            device,
            queue,
            color_format,
            opaque,
            transparent,
            globals,
            bind_group,
            meshes,
            opaque_batches: Vec::new(),
            batch_of: HashMap::new(),
            placements: Vec::new(),
            transparent_free: Vec::new(),
            transparent_buffer: None,
            transparent_parts: Vec::new(),
            lighting: Lighting::default(),
            depth: None,
        }
    }

    /// Draw the Head MeshType with this mesh (the user's own `head.mesh`)
    /// instead of Rook's. It must be the head at its own size, as Roblox's
    /// file is.
    pub fn set_head_mesh(&mut self, head: &rook_content::mesh::Mesh) {
        let gpu = GpuMesh::upload(&self.device, &mesh::from_file(head));
        self.meshes.insert(DrawShape::Head, gpu);
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

    /// Replace the scene. Opaque parts are grouped per shape (each mesh
    /// file its own group, drawn instanced) and uploaded once; transparent
    /// parts are kept for per-frame sorting.
    pub fn set_scene(&mut self, scene: &Scene) {
        profiling::scope!("Renderer::set_scene");
        self.opaque_batches.clear();
        self.batch_of.clear();
        self.placements.clear();
        self.transparent_parts.clear();
        self.transparent_free.clear();
        self.update_parts(scene, &(0..scene.parts.len()).collect::<Vec<_>>());
    }

    /// Update the parts at `changed` (indices into `scene.parts`) and the
    /// lighting. Indices must stay with their parts from one call to the
    /// next; the scene may grow, and a part that's no longer drawn should
    /// stay as a hidden one (fully transparent).
    pub fn update_parts(&mut self, scene: &Scene, changed: &[usize]) {
        profiling::scope!("Renderer::update_parts");
        self.lighting = scene.lighting;
        if self.placements.len() < scene.parts.len() {
            self.placements.resize(scene.parts.len(), Placement::Hidden);
        }
        for &i in changed {
            let part = &scene.parts[i];
            let target = if part.transparency >= 1.0 || !self.has_mesh(part.shape, scene) {
                Target::Hidden
            } else if part.transparency > 0.0 {
                Target::Transparent
            } else {
                Target::Opaque(part.shape)
            };
            match (self.placements[i], target) {
                (Placement::Hidden, Target::Hidden) => {}
                (Placement::Transparent(t), Target::Transparent) => {
                    self.transparent_parts[t] = Some(*part);
                }
                (Placement::Opaque { batch, slot }, Target::Opaque(shape))
                    if self.opaque_batches[batch].shape == shape =>
                {
                    self.opaque_batches[batch].set(slot, Instance::from_part(part));
                }
                (old, _) => {
                    self.vacate(old);
                    self.placements[i] = self.occupy(part, target);
                }
            }
        }
    }

    /// Free the slot a part was drawn in.
    fn vacate(&mut self, placement: Placement) {
        match placement {
            Placement::Hidden => {}
            Placement::Opaque { batch, slot } => {
                let b = &mut self.opaque_batches[batch];
                b.set(slot, Instance::zeroed());
                b.free.push(slot);
            }
            Placement::Transparent(t) => {
                self.transparent_parts[t] = None;
                self.transparent_free.push(t);
            }
        }
    }

    /// Take a slot to draw a part in.
    fn occupy(&mut self, part: &PartInstance, target: Target) -> Placement {
        match target {
            Target::Hidden => Placement::Hidden,
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
            Target::Opaque(shape) => {
                // Batches in the order their shapes first appear, so the
                // draw order is the same every run.
                let batch = *self.batch_of.entry(shape).or_insert_with(|| {
                    self.opaque_batches.push(Batch {
                        shape,
                        instances: Vec::new(),
                        free: Vec::new(),
                        buffer: None,
                        dirty: None,
                    });
                    self.opaque_batches.len() - 1
                });
                let b = &mut self.opaque_batches[batch];
                let slot = match b.free.pop() {
                    Some(slot) => slot,
                    None => {
                        b.instances.push(Instance::zeroed());
                        b.instances.len() - 1
                    }
                };
                b.set(slot, Instance::from_part(part));
                Placement::Opaque { batch, slot }
            }
        }
    }

    /// Whether a shape can be drawn, uploading a mesh file the first time
    /// one is used. A file the scene doesn't hold, or one without
    /// triangles, draws nothing.
    fn has_mesh(&mut self, shape: DrawShape, scene: &Scene) -> bool {
        if self.meshes.contains_key(&shape) {
            return true;
        }
        let DrawShape::File(hash) = shape else {
            return false;
        };
        let Some(file) = scene.meshes.get(&hash).filter(|m| !m.indices.is_empty()) else {
            return false;
        };
        profiling::scope!("upload mesh");
        let gpu = GpuMesh::upload(&self.device, &mesh::from_file(file));
        self.meshes.insert(shape, gpu);
        true
    }

    fn instance_buffer(&self, list: &[Instance]) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rook instances"),
                contents: bytemuck::cast_slice(list),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            })
    }

    /// A buffer holding `list` with room for `capacity` instances.
    fn instance_buffer_with_room(&self, list: &[Instance], capacity: usize) -> wgpu::Buffer {
        let mut padded = list.to_vec();
        padded.resize(capacity.max(list.len()), Instance::zeroed());
        self.instance_buffer(&padded)
    }

    /// Upload the opaque instances changed since the last frame, and the
    /// transparent ones sorted back to front for `camera`. Returns the
    /// transparent draws: (shape, instance range) runs in that order.
    fn upload(&mut self, camera: &Camera) -> Vec<(DrawShape, std::ops::Range<u32>)> {
        let size = std::mem::size_of::<Instance>();
        for i in 0..self.opaque_batches.len() {
            let b = &self.opaque_batches[i];
            let len = b.instances.len();
            match &b.buffer {
                Some((_, capacity)) if *capacity >= len => {
                    let b = &mut self.opaque_batches[i];
                    if let (Some(r), Some((buffer, _))) = (b.dirty.take(), &b.buffer) {
                        self.queue.write_buffer(
                            buffer,
                            (r.start * size) as u64,
                            bytemuck::cast_slice(&b.instances[r]),
                        );
                    }
                }
                // New, or grown past its buffer: room to grow again.
                _ => {
                    let capacity = len + len / 2 + 16;
                    let buffer = self.instance_buffer_with_room(&b.instances, capacity);
                    let b = &mut self.opaque_batches[i];
                    b.buffer = Some((buffer, capacity));
                    b.dirty = None;
                }
            }
        }
        // Back to front for blending.
        let mut order: Vec<&PartInstance> = self.transparent_parts.iter().flatten().collect();
        order.sort_by(|a, b| {
            let da = a.position.distance_squared(camera.position);
            let db = b.position.distance_squared(camera.position);
            db.total_cmp(&da)
        });
        let instances: Vec<Instance> = order.iter().map(|p| Instance::from_part(p)).collect();
        let mut runs: Vec<(DrawShape, std::ops::Range<u32>)> = Vec::new();
        for (i, p) in order.iter().enumerate() {
            let i = i as u32;
            match runs.last_mut() {
                Some((shape, r)) if *shape == p.shape => r.end = i + 1,
                _ => runs.push((p.shape, i..i + 1)),
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
                let buffer = self.instance_buffer_with_room(&instances, capacity);
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
    pub fn render(&mut self, target: &wgpu::TextureView, width: u32, height: u32, camera: &Camera) {
        profiling::scope!("Renderer::render");
        let l = self.lighting;
        let rgb = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        let globals = Globals {
            view_proj: camera
                .view_projection(width as f32 / height.max(1) as f32)
                .to_cols_array_2d(),
            camera_pos: rgb(camera.position.to_array(), 1.0),
            sun_dir: rgb(l.sun_direction.normalize_or(Vec3::Y).to_array(), 0.0),
            ambient: rgb(l.ambient, 0.0),
            outdoor_ambient: rgb(l.outdoor_ambient, 0.0),
            fog_color: rgb(l.fog_color, l.brightness),
            fog_range: [l.fog_start, l.fog_end, 0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let transparent_runs = self.upload(camera);

        let depth = self.depth_view(width, height);
        let sky = srgb_to_linear(l.sky_horizon);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rook frame"),
            });
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
            let opaque = self.opaque_batches.iter().filter_map(|b| {
                let (buffer, _) = b.buffer.as_ref()?;
                Some((b.shape, buffer, 0..b.instances.len() as u32))
            });
            let transparent =
                self.transparent_buffer
                    .as_ref()
                    .into_iter()
                    .flat_map(|(buffer, _)| {
                        transparent_runs
                            .iter()
                            .map(move |(shape, range)| (*shape, buffer, range.clone()))
                    });
            for (pipeline, draws) in [
                (&self.opaque, opaque.collect::<Vec<_>>()),
                (&self.transparent, transparent.collect::<Vec<_>>()),
            ] {
                pass.set_pipeline(pipeline);
                for (shape, buffer, instances) in draws {
                    // Parts are only drawn for shapes with a mesh.
                    let Some(mesh) = self.meshes.get(&shape) else {
                        continue;
                    };
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_vertex_buffer(1, buffer.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, instances);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
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
    use glam::Mat3;

    use super::*;

    fn part(shape: PartShape, x: f32, color: [f32; 3], transparency: f32) -> PartInstance {
        PartInstance {
            shape: DrawShape::Part(shape),
            position: Vec3::new(x, 0.0, 0.0),
            rotation: Mat3::IDENTITY,
            size: Vec3::splat(2.0),
            color,
            transparency,
        }
    }

    /// Updating parts one by one draws what setting the whole scene draws:
    /// moves, recolours, a shape change, opaque to transparent and back,
    /// a part hidden, and a new part in the slot it left.
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
        let mut scene = Scene {
            parts: vec![
                part(PartShape::Block, -6.0, red, 0.0),
                part(PartShape::Ball, -3.0, green, 0.0),
                part(PartShape::Block, 0.0, blue, 0.5),
                part(PartShape::Cylinder, 3.0, red, 0.0),
                part(PartShape::Block, 6.0, green, 0.0),
            ],
            ..Scene::default()
        };
        incremental.set_scene(&scene);
        let camera = Camera::look_at(Vec3::new(0.0, 3.0, 16.0), Vec3::ZERO);
        incremental.render_to_image(64, 32, &camera).unwrap();

        scene.parts[0].position.y = 1.0;
        scene.parts[0].color = blue;
        scene.parts[1].shape = DrawShape::Part(PartShape::Block);
        scene.parts[2].transparency = 0.0;
        scene.parts[3].transparency = 0.25;
        scene.parts[4].transparency = 1.0;
        scene.parts.push(part(PartShape::Wedge, 9.0, red, 0.0));
        incremental.update_parts(&scene, &[0, 1, 2, 3, 4, 5]);
        // The hidden part's slot is taken by a new one.
        scene.parts[4] = part(PartShape::Ball, -9.0, blue, 0.0);
        incremental.update_parts(&scene, &[4]);

        fresh.set_scene(&scene);
        assert_eq!(
            incremental.render_to_image(64, 32, &camera).unwrap(),
            fresh.render_to_image(64, 32, &camera).unwrap()
        );
    }
}
