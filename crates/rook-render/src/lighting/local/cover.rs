//! The sun cover: how much of the sun each light voxel stops, with mips
//! averaging each layer across x and z, which the fill pass traces the
//! sun's cones through (`sunlit` in `shaders/voxels/light_volume.wgsl`; built by
//! `shaders/voxels/sun_cover.wgsl`). Studio's voxel sun shadows soften with the
//! distance from what casts them (docs/behavior/lighting/sun.md), as voxel
//! cone tracing's do (Crassin et al., "Interactive Indirect Illumination
//! Using Voxel Cone Tracing", 2011): a cone sampling a mipmapped volume at
//! the level its width needs. Here each layer is sampled apart and a run
//! of blocking layers counts once, as in Studio, where a slab shadows the
//! same however thick it is. Only the tiles whose blockers changed are
//! built again.

use bytemuck::{Pod, Zeroable};
use wesl_core::include_wesl;

use super::blockers::Span;
use super::tiles::{ACROSS, TILE, WORDS};
use super::{VOLUME, buffer_entry};

/// The cover's levels, up to a tile across. The volume moves by whole
/// tiles (`snapped` in `local.rs`), so each texel of every level holds
/// voxels of one tile of the world.
pub(super) const LEVELS: u32 = 5;
const _: () = assert!(1 << (LEVELS - 1) == TILE);

/// The sun's cone, fitted to Studio 0.747's shadows of slabs 8 to 64
/// studs wide, 0.2 to 16 thick and 10 to 80 up
/// (docs/behavior/lighting/sun.md): its radius at the voxel it leaves, in
/// voxels...
pub(super) const CONE_START: f32 = 0.2;
/// ...and how much wider it gets per voxel it runs.
pub(super) const CONE_WIDENS: f32 = 0.2;

/// The cone's radius `t` voxels along it, at least `least` (`sunlit`).
pub(super) fn cone_radius(t: f32, least: f32) -> f32 {
    (CONE_START + CONE_WIDENS * t).max(least)
}

/// How far from its center a footprint of radius `r` reads the cover, in
/// voxels: the coarser level's texel, and its neighbor the sampler blends.
pub(super) fn footprint_reach(r: f32) -> f32 {
    let lod = (2.0 * r).log2().clamp(0.0, (LEVELS - 1) as f32);
    1.5 * lod.ceil().exp2() + 1.0
}

/// What one level's dispatch builds (`Job` in `shaders/voxels/sun_cover.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Job {
    /// xyz: voxels along each axis; w: the level.
    size: [u32; 4],
    /// xy: the first tile along x and z dispatched (kept).
    at: [u32; 4],
    /// A bit per tile (kept): whether it changed.
    tiles: [u32; WORDS],
}

/// Bytes between the levels' jobs (the alignment WebGPU allows dynamic
/// offsets at).
const JOB_STRIDE: u64 = 256;

/// The tiles of the cover to build again, as kept (the world's tile
/// modulo the volume's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Changed([u32; WORDS]);

impl Changed {
    pub(super) fn none() -> Self {
        Self([0; WORDS])
    }

    pub(super) fn all() -> Self {
        let mut all = Self::none();
        for t in 0..ACROSS[0] * ACROSS[1] {
            all.0[(t / 32) as usize] |= 1 << (t % 32);
        }
        all
    }

    /// Mark the tiles holding the voxels of `span` (the world's).
    pub(super) fn mark(&mut self, (lo, hi): Span) {
        let tile = TILE as i32;
        let tiles = |axis: usize, n: u32| {
            let (a, b) = (lo[axis].div_euclid(tile), hi[axis].div_euclid(tile));
            let n = n as i32;
            // A span across the whole volume reaches every tile once.
            let (a, b) = if b - a + 1 >= n { (0, n - 1) } else { (a, b) };
            (a..=b).map(move |t| t.rem_euclid(n) as u32)
        };
        for z in tiles(2, ACROSS[1]) {
            for x in tiles(0, ACROSS[0]) {
                let t = x + ACROSS[0] * z;
                self.0[(t / 32) as usize] |= 1 << (t % 32);
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }

    /// The first and last tile along x and z of the box around those
    /// marked.
    fn bounds(&self) -> ([u32; 2], [u32; 2]) {
        let (mut lo, mut hi) = ([u32::MAX; 2], [0; 2]);
        for t in 0..ACROSS[0] * ACROSS[1] {
            if self.0[(t / 32) as usize] & (1 << (t % 32)) != 0 {
                let at = [t % ACROSS[0], t / ACROSS[0]];
                for axis in 0..2 {
                    lo[axis] = lo[axis].min(at[axis]);
                    hi[axis] = hi[axis].max(at[axis]);
                }
            }
        }
        (lo, hi)
    }
}

/// The cover, its levels' views, and the passes that build it.
#[derive(Debug)]
pub(super) struct Cover {
    /// All of its levels, as the fill pass samples them.
    pub(super) view: wgpu::TextureView,
    /// Wrapping around as the voxels do, blending between levels.
    pub(super) sampler: wgpu::Sampler,
    jobs: wgpu::Buffer,
    /// Each level's: the level below it read, it written.
    groups: Vec<wgpu::BindGroup>,
    level0: wgpu::ComputePipeline,
    down: wgpu::ComputePipeline,
}

impl Cover {
    /// Built from the blockers' bits (`solid`) and the terrain's bytes
    /// (`soils`), as the fill pass reads them.
    pub(super) fn new(device: &wgpu::Device, solid: &wgpu::Buffer, soils: &wgpu::Buffer) -> Self {
        let [nx, ny, nz] = VOLUME;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rook sun cover"),
            size: wgpu::Extent3d {
                width: nx,
                height: nz,
                // Four layers to a texel.
                depth_or_array_layers: ny / 4,
            },
            mip_level_count: LEVELS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let level = |l: u32| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                base_mip_level: l,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let compute = wgpu::ShaderStages::COMPUTE;
        let read = wgpu::BufferBindingType::Storage { read_only: true };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook sun cover"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: compute,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(size_of::<Job>() as u64),
                    },
                    count: None,
                },
                buffer_entry(1, read, compute),
                buffer_entry(2, read, compute),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: compute,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: compute,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
            ],
        });
        let jobs = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rook sun cover jobs"),
            size: JOB_STRIDE * u64::from(LEVELS),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let groups = (0..LEVELS)
            .map(|l| {
                // Level 0 reads no level; any other than its own is bound.
                let below = level(if l == 0 { 1 } else { l - 1 });
                let made = level(l);
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rook sun cover level"),
                    layout: &layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &jobs,
                                offset: 0,
                                size: wgpu::BufferSize::new(size_of::<Job>() as u64),
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: solid.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: soils.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(&below),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(&made),
                        },
                    ],
                })
            })
            .collect();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook sun cover"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("sun_cover").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rook sun cover"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("rook sun cover"),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rook sun cover"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            }),
            sampler,
            jobs,
            groups,
            level0: pipeline("level0"),
            down: pipeline("down"),
        }
    }

    /// Build the tiles `changed` again from the blockers and the terrain
    /// as they are on the GPU, every level, before the fill pass reads
    /// them.
    pub(super) fn build(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        changed: &Changed,
    ) {
        if changed.is_empty() {
            return;
        }
        profiling::scope!("Cover::build");
        let [nx, ny, nz] = VOLUME;
        let (lo, hi) = changed.bounds();
        let mut bytes = vec![0u8; (JOB_STRIDE * u64::from(LEVELS)) as usize];
        for l in 0..LEVELS {
            let job = Job {
                size: [nx, ny, nz, l],
                at: [lo[0], lo[1], 0, 0],
                tiles: changed.0,
            };
            let at = (JOB_STRIDE * u64::from(l)) as usize;
            bytes[at..at + size_of::<Job>()].copy_from_slice(bytemuck::bytes_of(&job));
        }
        queue.write_buffer(&self.jobs, 0, &bytes);
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rook sun cover"),
            timestamp_writes: None,
        });
        for l in 0..LEVELS {
            pass.set_pipeline(if l == 0 { &self.level0 } else { &self.down });
            pass.set_bind_group(
                0,
                &self.groups[l as usize],
                &[(JOB_STRIDE * u64::from(l)) as u32],
            );
            let across = |axis: usize| ((hi[axis] - lo[axis] + 1) * (TILE >> l)).div_ceil(8);
            pass.dispatch_workgroups(across(0), across(1), ny / 4);
        }
    }
}
