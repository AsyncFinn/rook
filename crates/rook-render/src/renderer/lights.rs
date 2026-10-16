//! Local lights on the GPU, drawn as Studio's voxel lighting draws them
//! (docs/behavior/lights.md): every light in one buffer, a spatial hash
//! saying which lights reach each place, and a volume of 4-stud voxels
//! ahead of the camera that a compute pass fills from them
//! (`shaders/light_volume.wgsl`) and surfaces sample
//! (`shaders/lighting.wgsl`).
//!
//! The hash cuts the world into cubes `cell` studs across; each cube
//! hashes to a bucket, and a bucket lists every light whose reach overlaps
//! a cube that hashes to it (collisions only cost a few lights more to
//! test). Reaches are padded by [`SLACK`], so lights that move a little
//! (spinning fixtures) don't rebuild it. The voxels are filled again only
//! when a light changes or the camera leaves the volume's middle.

mod hash;

use wesl_core::include_wesl;

use super::*;
use hash::{GpuLight, LightHash};

/// How far a light may move from where the hash placed it before the hash
/// is built again, in studs.
pub(super) const SLACK: f32 = 8.0;
/// Studio's light voxels, in studs (centred at 4k + 2).
const VOXEL: f32 = 4.0;
/// The voxel volume: voxels along each axis (1024 x 256 x 1024 studs, as
/// Studio's grid: docs/behavior/lights.md). Light stops at its faces.
const VOLUME: [u32; 3] = [256, 64, 256];
/// How far ahead of the camera the volume's centre sits, along the view
/// direction flattened onto the ground (Studio's grid reaches about 760
/// studs ahead and 260 behind; looking straight down, it's centred).
const AHEAD: f32 = 256.0;
/// How far the volume's centre may drift from where it should be before
/// the volume follows, in studs.
const RECENTRE: f32 = 64.0;

/// What the fill pass reads (`Params` in `shaders/light_volume.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct FillParams {
    origin: [f32; 4],
    size: [u32; 4],
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
    /// The centre of voxel (0, 0, 0), once filled.
    origin: Option<Vec3>,
    /// The lights changed since the voxels were filled.
    dirty: bool,
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
        let fill_group = fill_group(
            device,
            &fill_layout,
            [&params, &light_buffer, &bucket_buffer, &entry_buffer],
            &voxels,
        );
        Self {
            layout,
            bind_group,
            lights: Vec::new(),
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
            origin: None,
            dirty: true,
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
        let mut lights: Vec<GpuLight> = scene.iter().map(GpuLight::of).collect();
        if lights == self.lights {
            return;
        }
        profiling::scope!("GpuLights::update");
        self.dirty = true;
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
            &self.voxels,
        );
    }

    /// Fill the voxels again if the lights changed or the volume's centre
    /// drifted [`RECENTRE`] from where the camera puts it.
    pub(crate) fn fill(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: &Camera,
    ) {
        let dims = Vec3::new(VOLUME[0] as f32, VOLUME[1] as f32, VOLUME[2] as f32);
        let half = dims * (VOXEL * 0.5);
        let forward = camera.forward();
        let centre = camera.position + Vec3::new(forward.x, 0.0, forward.z) * AHEAD;
        let centred = self
            .origin
            .is_some_and(|o| (o + half - centre).abs().max_element() <= RECENTRE);
        if !self.dirty && centred {
            return;
        }
        profiling::scope!("GpuLights::fill");
        self.dirty = false;
        // Voxels are centred at 4k + 2.
        let origin = ((centre - half) / VOXEL).floor() * VOXEL + Vec3::splat(VOXEL * 0.5);
        self.origin = Some(origin);
        let lit = !self.lights.is_empty();
        queue.write_buffer(
            &self.volume,
            0,
            bytemuck::bytes_of(&VolumeUniform {
                origin: [origin.x, origin.y, origin.z, f32::from(u8::from(lit))],
                size: [dims.x, dims.y, dims.z, 0.0],
            }),
        );
        if !lit {
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
            }),
        );
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rook light volume fill"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.fill);
        pass.set_bind_group(0, &self.fill_group, &[]);
        pass.dispatch_workgroups(VOLUME[0] / 4, VOLUME[1] / 4, VOLUME[2] / 4);
    }
}

/// The lights' bytes; an empty list still binds one (unlit) light.
fn light_bytes(lights: &[GpuLight]) -> &[u8] {
    const NONE: [GpuLight; 1] = [GpuLight {
        position: [0.0; 4],
        color: [0.0; 4],
        direction: [0.0; 4],
        kind: [0.0; 4],
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
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rook light volume fill"),
        layout,
        entries: &entries,
    })
}
