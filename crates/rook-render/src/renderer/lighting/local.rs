//! Local lights on the GPU, drawn as Studio's voxel lighting draws them:
//! every light in one buffer, a spatial hash saying which lights reach each
//! place (reaches padded by [`SLACK`], so lights that move a little don't
//! rebuild it), and a volume of 4-stud voxels ahead of the camera that a
//! compute pass fills (`shaders/light_volume.wgsl`) and surfaces sample
//! (`shaders/lighting.wgsl`). The voxels are filled again only when a light
//! changes or the camera leaves the volume's middle; parts marked into them
//! (`blockers.rs`) carry the sky's light down and shadow lights. What Studio
//! does is in `docs/behavior/lighting/local-lights.md`.

mod blockers;
mod hash;

use wesl_core::include_wesl;

use super::*;
use blockers::Blockers;
use hash::{GpuLight, LightHash};

/// How far a light may move from where the hash placed it before the hash
/// is built again, in studs.
pub(super) const SLACK: f32 = 8.0;
/// Studio's light voxels, in studs (centered at 4k + 2).
const VOXEL: f32 = 4.0;
/// The voxel volume: voxels along each axis (1024 x 256 x 1024 studs, as
/// Studio's grid: docs/behavior/lighting/local-lights.md). Light stops at its faces.
const VOLUME: [u32; 3] = [256, 64, 256];
/// How far ahead of the camera the volume's center sits, along the view
/// direction flattened onto the ground (Studio's grid reaches about 760
/// studs ahead and 260 behind; looking straight down, it's centered).
const AHEAD: f32 = 256.0;
/// How far the volume's center may drift from where it should be before
/// the volume follows, in studs.
const RECENTER: f32 = 64.0;

/// Bytes between the layers' entries in [`SkyBuffers::layers`] (the
/// alignment WebGPU allows dynamic offsets at).
const LAYER_STRIDE: u64 = 256;

/// What the fill pass reads (`Params` in `shaders/light_volume.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct FillParams {
    origin: [f32; 4],
    size: [u32; 4],
    /// x: there are lights to fill; y: the sky's light to carry down; z:
    /// the blockers are marked (some light has `Shadows`).
    flags: [u32; 4],
}

/// The buffers the fill pass carries the sky's light down with.
#[derive(Debug)]
struct SkyBuffers {
    /// Each dispatch's layer, [`LAYER_STRIDE`] apart (a dynamic offset).
    layers: wgpu::Buffer,
    /// One bit per voxel: a part blocks it ([`Blockers::mark`]).
    solid: wgpu::Buffer,
    /// The sky's light in the last two layers filled.
    rows: wgpu::Buffer,
    /// Whether each column is still open to the sky.
    columns: wgpu::Buffer,
}

/// Where the main pass finds the voxels (`LightVolume` in
/// `shaders/lighting.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct VolumeUniform {
    origin: [f32; 4],
    size: [f32; 4],
}

/// The lights' buffers and hash, the voxel volume and the pass that fills
/// it, and the main pass's bind group (group 3).
#[derive(Debug)]
pub(crate) struct GpuLights {
    pub(crate) layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    lights: Vec<GpuLight>,
    /// The scene's lights they were made from, so an unchanged list
    /// skips making them again.
    scene_lights: Vec<LocalLight>,
    /// Where each light was when the hash was built.
    hashed_at: Vec<Vec3>,
    hash: LightHash,
    light_buffer: wgpu::Buffer,
    bucket_buffer: wgpu::Buffer,
    entry_buffer: wgpu::Buffer,
    params: wgpu::Buffer,
    volume: wgpu::Buffer,
    voxels: wgpu::TextureView,
    fill: wgpu::ComputePipeline,
    fill_layout: wgpu::BindGroupLayout,
    fill_group: wgpu::BindGroup,
    sky_buffers: SkyBuffers,
    blockers: Blockers,
    /// The center of voxel (0, 0, 0), once filled.
    origin: Option<Vec3>,
    /// The lights changed since the voxels were filled.
    dirty: bool,
    /// The voxels hold the sky's light.
    sky: bool,
    /// Some light has `Shadows`.
    shadows: bool,
    /// The blockers are marked for the volume where it is now.
    marked: bool,
}

fn buffer_entry(
    binding: u32,
    ty: wgpu::BufferBindingType,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

impl SkyBuffers {
    fn new(device: &wgpu::Device) -> Self {
        let [nx, ny, nz] = VOLUME;
        let storage = |label, words: u32, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: u64::from(words) * 4,
                usage: wgpu::BufferUsages::STORAGE | usage,
                mapped_at_creation: false,
            })
        };
        let mut layers = vec![0u8; (LAYER_STRIDE * u64::from(ny)) as usize];
        for y in 0..ny {
            let at = (LAYER_STRIDE * u64::from(y)) as usize;
            layers[at..at + 4].copy_from_slice(&y.to_le_bytes());
        }
        Self {
            layers: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rook light volume layers"),
                contents: &layers,
                usage: wgpu::BufferUsages::UNIFORM,
            }),
            solid: storage(
                "rook light blockers",
                (nx * ny * nz).div_ceil(32),
                wgpu::BufferUsages::COPY_DST,
            ),
            rows: storage("rook sky rows", 2 * nx * nz, wgpu::BufferUsages::empty()),
            columns: storage("rook sky columns", nx * nz, wgpu::BufferUsages::empty()),
        }
    }
}

impl GpuLights {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook light volume"),
            entries: &[
                buffer_entry(0, wgpu::BufferBindingType::Uniform, fragment),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: fragment,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: fragment,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let compute = wgpu::ShaderStages::COMPUTE;
        let storage = wgpu::BufferBindingType::Storage { read_only: true };
        let read_write = wgpu::BufferBindingType::Storage { read_only: false };
        let fill_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook light volume fill"),
            entries: &[
                buffer_entry(0, wgpu::BufferBindingType::Uniform, compute),
                buffer_entry(1, storage, compute),
                buffer_entry(2, storage, compute),
                buffer_entry(3, storage, compute),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: compute,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D3,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: compute,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
                buffer_entry(6, storage, compute),
                buffer_entry(7, read_write, compute),
                buffer_entry(8, read_write, compute),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook light volume"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("light_volume").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rook light volume fill"),
            bind_group_layouts: &[Some(&fill_layout)],
            immediate_size: 0,
        });
        let fill = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rook light volume fill"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("fill"),
            compilation_options: Default::default(),
            cache: None,
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rook light voxels"),
            size: wgpu::Extent3d {
                width: VOLUME[0],
                height: VOLUME[1],
                depth_or_array_layers: VOLUME[2],
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let voxels = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rook light voxels"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = |label, size: usize| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let volume = uniform("rook light volume", std::mem::size_of::<VolumeUniform>());
        let params = uniform("rook light volume fill", std::mem::size_of::<FillParams>());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook light volume"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: volume.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&voxels),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let hash = LightHash::build(&[]);
        let (light_buffer, bucket_buffer, entry_buffer) = buffers(device, &[], &hash);
        let sky_buffers = SkyBuffers::new(device);
        let fill_group = fill_group(
            device,
            &fill_layout,
            [&params, &light_buffer, &bucket_buffer, &entry_buffer],
            &sky_buffers,
            &voxels,
        );
        Self {
            layout,
            bind_group,
            lights: Vec::new(),
            scene_lights: Vec::new(),
            hashed_at: Vec::new(),
            hash,
            light_buffer,
            bucket_buffer,
            entry_buffer,
            params,
            volume,
            voxels,
            fill,
            fill_layout,
            fill_group,
            sky_buffers,
            blockers: Blockers::default(),
            origin: None,
            dirty: true,
            sky: false,
            shadows: false,
            marked: false,
        }
    }

    /// Bring the GPU's lights up to date with the scene's. The hash is
    /// built again only when lights come or go, or one moves past
    /// [`SLACK`] from where it was hashed or reaches further.
    pub(crate) fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &[LocalLight],
    ) {
        if scene == self.scene_lights {
            return;
        }
        self.scene_lights = scene.to_vec();
        let mut lights: Vec<GpuLight> = scene.iter().map(GpuLight::of).collect();
        if lights == self.lights {
            return;
        }
        profiling::scope!("GpuLights::update");
        self.dirty = true;
        self.shadows = lights.iter().any(GpuLight::shadows);
        let hashed = lights.len() == self.lights.len()
            && lights
                .iter()
                .zip(&self.lights)
                .zip(&self.hashed_at)
                .all(|((a, b), at)| {
                    a.position[3] <= b.position[3] && a.at().distance(*at) <= SLACK
                });
        if hashed {
            queue.write_buffer(&self.light_buffer, 0, light_bytes(&lights));
            self.lights = lights;
            return;
        }
        self.hash = LightHash::build(&lights);
        self.hashed_at = lights.iter().map(GpuLight::at).collect();
        std::mem::swap(&mut self.lights, &mut lights);
        (self.light_buffer, self.bucket_buffer, self.entry_buffer) =
            buffers(device, &self.lights, &self.hash);
        self.fill_group = fill_group(
            device,
            &self.fill_layout,
            [
                &self.params,
                &self.light_buffer,
                &self.bucket_buffer,
                &self.entry_buffer,
            ],
            &self.sky_buffers,
            &self.voxels,
        );
    }

    /// Forget the parts blocking light (the scene is replaced).
    pub(crate) fn clear_blockers(&mut self) {
        self.blockers.clear();
    }

    /// Take the parts at `changed` (indices into `scene.parts`) as they
    /// block light now. Only noted while nothing reads the blockers (the
    /// sky lights nothing and no light has `Shadows`), and taken once
    /// something does.
    pub(crate) fn update_blockers(&mut self, scene: &Scene, changed: &[usize]) {
        let used = scene.lighting.environment_diffuse > 0.0 || self.shadows;
        self.blockers.update(scene, changed, used);
    }

    /// Fill the voxels again if the lights changed, the volume's center
    /// drifted [`RECENTER`] from where the camera puts it, or, when the
    /// place lights with the sky (`sky`) or a light has `Shadows`, the
    /// parts blocking light changed.
    pub(crate) fn fill(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: &Camera,
        sky: bool,
    ) {
        let dims = Vec3::new(VOLUME[0] as f32, VOLUME[1] as f32, VOLUME[2] as f32);
        let half = dims * (VOXEL * 0.5);
        let forward = camera.forward();
        let center = camera.position + Vec3::new(forward.x, 0.0, forward.z) * AHEAD;
        let centered = self
            .origin
            .is_some_and(|o| (o + half - center).abs().max_element() <= RECENTER);
        let solid = sky || self.shadows;
        let blocked = solid && (self.blockers.due() || !self.marked);
        if !self.dirty && centered && !blocked && sky == self.sky {
            return;
        }
        profiling::scope!("GpuLights::fill");
        self.dirty = false;
        let origin = if centered {
            self.origin.unwrap_or_default()
        } else {
            // Voxels are centered at 4k + 2.
            ((center - half) / VOXEL).floor() * VOXEL + Vec3::splat(VOXEL * 0.5)
        };
        if solid && (blocked || !centered) {
            let bits = self.blockers.mark(origin, VOLUME);
            queue.write_buffer(&self.sky_buffers.solid, 0, bytemuck::cast_slice(bits));
        }
        self.marked = solid;
        self.origin = Some(origin);
        self.sky = sky;
        let lit = !self.lights.is_empty();
        queue.write_buffer(
            &self.volume,
            0,
            bytemuck::bytes_of(&VolumeUniform {
                origin: [origin.x, origin.y, origin.z, f32::from(u8::from(lit))],
                size: [dims.x, dims.y, dims.z, f32::from(u8::from(sky))],
            }),
        );
        if !lit && !sky {
            return;
        }
        queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&FillParams {
                origin: [origin.x, origin.y, origin.z, self.hash.cell],
                size: [
                    self.hash.buckets.len() as u32 - 1,
                    VOLUME[0],
                    VOLUME[1],
                    VOLUME[2],
                ],
                flags: [u32::from(lit), u32::from(sky), u32::from(solid), 0],
            }),
        );
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rook light volume fill"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.fill);
        // One layer at a time from the top: each takes the sky's light
        // from the one above.
        for y in (0..VOLUME[1]).rev() {
            let offset = (LAYER_STRIDE * u64::from(y)) as u32;
            pass.set_bind_group(0, &self.fill_group, &[offset]);
            pass.dispatch_workgroups(VOLUME[0] / 8, VOLUME[2] / 8, 1);
        }
    }
}

/// The lights' bytes; an empty list still binds one (unlit) light.
fn light_bytes(lights: &[GpuLight]) -> &[u8] {
    const NONE: [GpuLight; 1] = [GpuLight {
        position: [0.0; 4],
        color: [0.0; 4],
        direction: [0.0; 4],
        kind: [0.0; 4],
        part_min: [0.0; 4],
        part_max: [0.0; 4],
    }];
    if lights.is_empty() {
        bytemuck::cast_slice(&NONE)
    } else {
        bytemuck::cast_slice(lights)
    }
}

fn buffers(
    device: &wgpu::Device,
    lights: &[GpuLight],
    hash: &LightHash,
) -> (wgpu::Buffer, wgpu::Buffer, wgpu::Buffer) {
    let make = |label, contents: &[u8]| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        })
    };
    // Storage buffers can't be empty.
    let entries: &[u32] = if hash.entries.is_empty() {
        &[0]
    } else {
        &hash.entries
    };
    (
        make("rook lights", light_bytes(lights)),
        make("rook light buckets", bytemuck::cast_slice(&hash.buckets)),
        make("rook light entries", bytemuck::cast_slice(entries)),
    )
}

fn fill_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffers: [&wgpu::Buffer; 4],
    sky: &SkyBuffers,
    voxels: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let mut entries: Vec<_> = buffers
        .iter()
        .enumerate()
        .map(|(i, b)| wgpu::BindGroupEntry {
            binding: i as u32,
            resource: b.as_entire_binding(),
        })
        .collect();
    entries.push(wgpu::BindGroupEntry {
        binding: 4,
        resource: wgpu::BindingResource::TextureView(voxels),
    });
    entries.push(wgpu::BindGroupEntry {
        binding: 5,
        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: &sky.layers,
            offset: 0,
            size: wgpu::BufferSize::new(16),
        }),
    });
    for (binding, buffer) in [(6, &sky.solid), (7, &sky.rows), (8, &sky.columns)] {
        entries.push(wgpu::BindGroupEntry {
            binding,
            resource: buffer.as_entire_binding(),
        });
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rook light volume fill"),
        layout,
        entries: &entries,
    })
}
