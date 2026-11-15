//! The GPU side of the light volume: its buffers and textures, the fill
//! pass's and the main pass's bind groups, and reading the voxels back.

use super::*;

/// Bytes between the layers' entries in [`SkyBuffers::layers`] (the
/// alignment WebGPU allows dynamic offsets at).
pub(super) const LAYER_STRIDE: u64 = 256;

/// What one dispatch of the fill pass fills (`Layer` in
/// `shaders/light_volume.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(super) struct LayerUniform {
    /// x: the layer; y, z: the first tile along x and z dispatched.
    pub(super) at: [u32; 4],
    /// A bit per tile of the layer: whether to fill it.
    pub(super) tiles: [u32; WORDS],
}

pub(super) const LAYER_SIZE: u64 = std::mem::size_of::<LayerUniform>() as u64;

/// What the fill pass reads (`Params` in `shaders/light_volume.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(super) struct FillParams {
    pub(super) origin: [f32; 4],
    pub(super) size: [u32; 4],
    /// Where voxel (0, 0, 0) is kept in the blockers' bits and fills, the
    /// sky's light and the voxels: all anchored to the world (voxel `k`
    /// at `k` modulo the volume's size, as `Blockers::mark` keeps them),
    /// so a volume that moves keeps what it still covers and fills only
    /// what it came onto, as Studio's light map wraps around
    /// (docs/behavior/lighting/local-lights.md).
    pub(super) ring: [u32; 4],
    /// x: there are lights to fill; y: the sky's light to carry down; z:
    /// the blockers are marked; w: the sun's light to take.
    pub(super) flags: [u32; 4],
    /// Towards the sun (unit).
    pub(super) sun: [f32; 4],
    /// The sun's cone: x: [`CONE_START`]; y: [`CONE_WIDENS`]; z: the sun
    /// cover's last level.
    pub(super) cone: [f32; 4],
}

/// The buffers the fill pass carries the sky's light down with.
#[derive(Debug)]
pub(super) struct SkyBuffers {
    /// Each dispatch's [`LayerUniform`], [`LAYER_STRIDE`] apart (a dynamic
    /// offset).
    pub(super) layers: wgpu::Buffer,
    /// One bit per voxel: a part blocks it ([`Blockers::mark`]).
    pub(super) solid: wgpu::Buffer,
    /// A byte per voxel: how full of solid terrain it is.
    pub(super) soils: wgpu::Buffer,
    /// The sky's light in each voxel, kept for the next fill of some
    /// tiles to take the layers above from.
    pub(super) skies: wgpu::Buffer,
}

/// Where the main pass finds the voxels (`LightVolume` in
/// `shaders/lighting.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(super) struct VolumeUniform {
    pub(super) origin: [f32; 4],
    pub(super) size: [f32; 4],
    /// x: 1 when the sun voxels hold how much of the sun reaches them.
    pub(super) sun: [f32; 4],
    /// xyz: where voxel (0, 0, 0) is kept ([`FillParams::ring`]).
    pub(super) ring: [f32; 4],
}

pub(super) fn buffer_entry(
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
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let [nx, ny, nz] = VOLUME;
        let storage = |label, words: u32, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: u64::from(words) * 4,
                usage: wgpu::BufferUsages::STORAGE | usage,
                mapped_at_creation: false,
            })
        };
        Self {
            layers: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rook light volume layers"),
                size: LAYER_STRIDE * u64::from(ny),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            solid: storage(
                "rook light blockers",
                (nx * ny * nz).div_ceil(32),
                wgpu::BufferUsages::COPY_DST,
            ),
            soils: storage(
                "rook light terrain",
                (nx * ny * nz).div_ceil(4),
                wgpu::BufferUsages::COPY_DST,
            ),
            skies: storage("rook sky voxels", nx * ny * nz, wgpu::BufferUsages::empty()),
        }
    }
}

/// The light group's entries: where the voxels are, the light and sun
/// voxels, and how they're sampled.
pub(super) fn group_entries<'a>(
    volume: &'a wgpu::Buffer,
    [voxels, sun_voxels]: [&'a wgpu::TextureView; 2],
    sampler: &'a wgpu::Sampler,
) -> [wgpu::BindGroupEntry<'a>; 4] {
    [
        wgpu::BindGroupEntry {
            binding: 0,
            resource: volume.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(voxels),
        },
        wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::Sampler(sampler),
        },
        wgpu::BindGroupEntry {
            binding: 3,
            resource: wgpu::BindingResource::TextureView(sun_voxels),
        },
    ]
}

/// The lights' bytes; an empty list still binds one (unlit) light.
pub(super) fn light_bytes(lights: &[GpuLight]) -> &[u8] {
    pub(super) const NONE: [GpuLight; 1] = [GpuLight {
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

pub(super) fn buffers(
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

/// The fill pass's bind group; `voxels` are the light voxels and the sun
/// voxels.
pub(super) fn fill_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffers: [&wgpu::Buffer; 4],
    (sky, cover): (&SkyBuffers, &Cover),
    [voxels, sun_voxels]: [&wgpu::TextureView; 2],
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
            size: wgpu::BufferSize::new(LAYER_SIZE),
        }),
    });
    for (binding, buffer) in [(6, &sky.solid), (7, &sky.skies), (10, &sky.soils)] {
        entries.push(wgpu::BindGroupEntry {
            binding,
            resource: buffer.as_entire_binding(),
        });
    }
    entries.push(wgpu::BindGroupEntry {
        binding: 9,
        resource: wgpu::BindingResource::TextureView(&cover.view),
    });
    entries.push(wgpu::BindGroupEntry {
        binding: 11,
        resource: wgpu::BindingResource::Sampler(&cover.sampler),
    });
    entries.push(wgpu::BindGroupEntry {
        binding: 8,
        resource: wgpu::BindingResource::TextureView(sun_voxels),
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rook light volume fill"),
        layout,
        entries: &entries,
    })
}

impl GpuLights {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook light volume"),
            entries: &Self::layout_entries(),
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
                        min_binding_size: wgpu::BufferSize::new(LAYER_SIZE),
                    },
                    count: None,
                },
                buffer_entry(6, storage, compute),
                buffer_entry(7, read_write, compute),
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: compute,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D3,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: compute,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                buffer_entry(10, storage, compute),
                wgpu::BindGroupLayoutEntry {
                    binding: 11,
                    visibility: compute,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
        let volume_texture = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: VOLUME[0],
                        height: VOLUME[1],
                        depth_or_array_layers: VOLUME[2],
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D3,
                    format,
                    usage: wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        // Tests read them back.
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let voxels = volume_texture("rook light voxels", wgpu::TextureFormat::Rgba16Float);
        let sun_voxels = volume_texture("rook sun voxels", wgpu::TextureFormat::Rgba8Unorm);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rook light voxels"),
            // The voxels wrap around (`FillParams::ring`).
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
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
            entries: &group_entries(&volume, [&voxels, &sun_voxels], &sampler),
        });
        // Zeroed: `origin.w`, `size.w` and `sun.x` 0 say there's no volume.
        let no_volume = uniform("rook no light volume", std::mem::size_of::<VolumeUniform>());
        let unlit = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook no light volume"),
            layout: &layout,
            entries: &group_entries(&no_volume, [&voxels, &sun_voxels], &sampler),
        });
        let hash = LightHash::build(&[]);
        let (light_buffer, bucket_buffer, entry_buffer) = buffers(device, &[], &hash);
        let sky_buffers = SkyBuffers::new(device);
        let cover = Cover::new(device, &sky_buffers.solid, &sky_buffers.soils);
        let fill_group = fill_group(
            device,
            &fill_layout,
            [&params, &light_buffer, &bucket_buffer, &entry_buffer],
            (&sky_buffers, &cover),
            [&voxels, &sun_voxels],
        );
        Self {
            layout,
            bind_group,
            unlit,
            lights: Vec::new(),
            shadowed: 0,
            hashed_at: Vec::new(),
            hash,
            light_buffer,
            bucket_buffer,
            entry_buffer,
            params,
            volume,
            voxels,
            sun_voxels,
            sampler,
            fill,
            fill_layout,
            fill_group,
            sky_buffers,
            cover,
            blockers: Blockers::default(),
            origin: None,
            dirty: Dirty {
                all: true,
                lights: Vec::new(),
            },
            sky: false,
            shadows: false,
            sun: None,
            marked: false,
            filled: 0,
        }
    }

    /// The light group's layout entries (bindings 0 to 3), which other
    /// groups bound in its place (the world GUI's) start with.
    pub(crate) fn layout_entries() -> [wgpu::BindGroupLayoutEntry; 4] {
        let fragment = wgpu::ShaderStages::FRAGMENT;
        [
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
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: fragment,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D3,
                    multisampled: false,
                },
                count: None,
            },
        ]
    }

    /// The light group's resources, at [`GpuLights::layout_entries`]'s
    /// bindings.
    pub(crate) fn entries(&self) -> [wgpu::BindGroupEntry<'_>; 4] {
        group_entries(
            &self.volume,
            [&self.voxels, &self.sun_voxels],
            &self.sampler,
        )
    }

    /// Every voxel's bytes, of the light voxels then the sun voxels, as
    /// kept (at slots anchored to the world).
    #[cfg(test)]
    pub(crate) fn read_voxels(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> [Vec<u8>; 2] {
        [&self.voxels, &self.sun_voxels].map(|view| {
            let texture = view.texture();
            let texel = texture
                .format()
                .block_copy_size(None)
                .expect("a plain format");
            let row = VOLUME[0] * texel;
            let size = u64::from(row * VOLUME[1] * VOLUME[2]);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rook light voxels readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row),
                        rows_per_image: Some(VOLUME[1]),
                    },
                },
                texture.size(),
            );
            queue.submit([encoder.finish()]);
            let slice = buffer.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .expect("the copy finishes");
            slice.get_mapped_range().expect("mapped").to_vec()
        })
    }
}
