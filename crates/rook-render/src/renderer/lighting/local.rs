//! Local lights on the GPU, drawn as Studio's voxel lighting draws them:
//! every light in one buffer, a spatial hash saying which lights reach each
//! place (reaches padded by [`SLACK`], so lights that move a little don't
//! rebuild it), and a volume of 4-stud voxels ahead of the camera that a
//! compute pass fills (`shaders/light_volume.wgsl`) and surfaces sample
//! (`shaders/lighting.wgsl`). The voxels are filled again only where a
//! light or a part changed (`tiles.rs`), and, when the camera leaves the
//! volume's middle and it follows, only what it came onto and what that
//! reaches: they are kept anchored to the world, wrapping around as
//! Studio's light map does. Parts marked into them (`blockers.rs`) carry the
//! sky's light down, shadow lights and, without
//! `PrioritizeLightingQuality`, shadow the sun. What Studio does is in
//! `docs/behavior/lighting/local-lights.md` and `docs/behavior/lighting/sun.md`.

mod blockers;
mod hash;
mod tiles;

use wesl_core::include_wesl;

use super::*;
use blockers::{Blockers, Span, Touched};
use glam::{IVec2, IVec3};
use hash::{GpuLight, LightHash};
use tiles::{ACROSS, TILE, Tiles, WORDS};

/// How far a light may move from where the hash placed it before the hash
/// is built again, in studs.
pub(super) const SLACK: f32 = 8.0;
/// Studio's light voxels, in studs (centered at 4k + 2).
const VOXEL: f32 = 4.0;
/// The voxel volume: voxels along each axis (1024 x 256 x 1024 studs, as
/// Studio's grid: docs/behavior/lighting/local-lights.md). Light stops at its faces.
const VOLUME: [u32; 3] = [256, 64, 256];
/// How far ahead of the camera's Focus the volume's center sits, along the
/// view direction with its height dropped (Studio's grid reaches about 760
/// studs ahead of its edit camera, whose Focus is 20 ahead, and 260 behind;
/// looking straight down, it's centered).
const AHEAD: f32 = 236.0;
/// How far the volume's center may drift from where it should be before
/// the volume follows, in studs.
const RECENTER: f32 = 64.0;
/// How far the sun may turn (the cosine of the angle) before the voxels
/// take its light again; a script turning `ClockTime` would otherwise
/// fill them every frame.
const SUN_TURN: f32 = 0.99995;

/// Bytes between the layers' entries in [`SkyBuffers::layers`] (the
/// alignment WebGPU allows dynamic offsets at).
const LAYER_STRIDE: u64 = 256;

/// What one dispatch of the fill pass fills (`Layer` in
/// `shaders/light_volume.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct LayerUniform {
    /// x: the layer; y, z: the first tile along x and z dispatched.
    at: [u32; 4],
    /// A bit per tile of the layer: whether to fill it.
    tiles: [u32; WORDS],
}

/// A [`LayerUniform`]'s bytes.
const LAYER_SIZE: u64 = std::mem::size_of::<LayerUniform>() as u64;

/// What the fill pass reads (`Params` in `shaders/light_volume.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct FillParams {
    origin: [f32; 4],
    size: [u32; 4],
    /// Where voxel (0, 0, 0) is kept in the blockers' bits and fills, the
    /// sky's light and the voxels: all anchored to the world (voxel `k`
    /// at `k` modulo the volume's size, as `Blockers::mark` keeps them),
    /// so a volume that moves keeps what it still covers and fills only
    /// what it came onto, as Studio's light map wraps around
    /// (docs/behavior/lighting/local-lights.md).
    ring: [u32; 4],
    /// x: there are lights to fill; y: the sky's light to carry down; z:
    /// the blockers are marked; w: the sun's light to take.
    flags: [u32; 4],
    /// Towards the sun (unit).
    sun: [f32; 4],
}

/// The buffers the fill pass carries the sky's light down with.
#[derive(Debug)]
struct SkyBuffers {
    /// Each dispatch's [`LayerUniform`], [`LAYER_STRIDE`] apart (a dynamic
    /// offset).
    layers: wgpu::Buffer,
    /// One bit per voxel: a part blocks it ([`Blockers::mark`]).
    solid: wgpu::Buffer,
    /// A byte per voxel: how much of it parts fill.
    fills: wgpu::Buffer,
    /// The sky's light in each voxel, kept for the next fill of some
    /// tiles to take the layers above from.
    skies: wgpu::Buffer,
}

/// What changed since the voxels were filled, besides the blockers.
#[derive(Debug, Default)]
struct Dirty {
    /// The lights were hashed again: everything.
    all: bool,
    /// The voxels lights that changed reach, before and after (the
    /// world's voxels).
    lights: Vec<Span>,
}

impl Dirty {
    fn is_empty(&self) -> bool {
        !self.all && self.lights.is_empty()
    }
}

/// Where the main pass finds the voxels (`LightVolume` in
/// `shaders/lighting.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct VolumeUniform {
    origin: [f32; 4],
    size: [f32; 4],
    /// x: 1 when the sun voxels hold how much of the sun reaches them.
    sun: [f32; 4],
    /// xyz: where voxel (0, 0, 0) is kept ([`FillParams::ring`]).
    ring: [f32; 4],
}

/// The lights' buffers and hash, the voxel volume and the pass that fills
/// it, and the main pass's bind group (group 3).
#[derive(Debug)]
pub(crate) struct GpuLights {
    pub(crate) layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    lights: Vec<GpuLight>,
    /// How many of `lights` have `Shadows`.
    shadowed: usize,
    /// Where each light was when the hash was built.
    hashed_at: Vec<Vec3>,
    hash: LightHash,
    light_buffer: wgpu::Buffer,
    bucket_buffer: wgpu::Buffer,
    entry_buffer: wgpu::Buffer,
    params: wgpu::Buffer,
    volume: wgpu::Buffer,
    voxels: wgpu::TextureView,
    /// How much of the sun reaches each voxel (red), past the parts.
    sun_voxels: wgpu::TextureView,
    sampler: wgpu::Sampler,
    fill: wgpu::ComputePipeline,
    fill_layout: wgpu::BindGroupLayout,
    fill_group: wgpu::BindGroup,
    sky_buffers: SkyBuffers,
    blockers: Blockers,
    /// The center of voxel (0, 0, 0), once filled.
    origin: Option<Vec3>,
    /// What changed since the voxels were filled, besides the blockers.
    dirty: Dirty,
    /// The voxels hold the sky's light.
    sky: bool,
    /// Some light has `Shadows`.
    shadows: bool,
    /// The sun (towards it) the voxels were filled with, if they hold its
    /// light.
    sun: Option<Vec3>,
    /// The blockers are marked for the volume where it is now.
    marked: bool,
    /// The tiles the last fill filled, over every layer.
    filled: u32,
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
            fills: storage(
                "rook light blocker fills",
                (nx * ny * nz).div_ceil(4),
                wgpu::BufferUsages::COPY_DST,
            ),
            skies: storage("rook sky voxels", nx * ny * nz, wgpu::BufferUsages::empty()),
        }
    }
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
                buffer_entry(9, storage, compute),
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
        let hash = LightHash::build(&[]);
        let (light_buffer, bucket_buffer, entry_buffer) = buffers(device, &[], &hash);
        let sky_buffers = SkyBuffers::new(device);
        let fill_group = fill_group(
            device,
            &fill_layout,
            [&params, &light_buffer, &bucket_buffer, &entry_buffer],
            &sky_buffers,
            [&voxels, &sun_voxels],
        );
        Self {
            layout,
            bind_group,
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

    /// Take a new scene's lights.
    pub(crate) fn replace(&mut self, device: &wgpu::Device, scene: &[LocalLight]) {
        self.lights = scene.iter().map(GpuLight::of).collect();
        self.shadowed = self.lights.iter().filter(|l| l.shadows()).count();
        self.rehash(device);
    }

    /// Take the lights at `changed` (indices into `scene`, which may have
    /// grown or shrunk) again. Only they are written; the hash is built
    /// again only when lights come or go, or one moves past [`SLACK`] from
    /// where it was hashed or reaches further.
    pub(crate) fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &[LocalLight],
        changed: &[usize],
    ) {
        let mut rehash = scene.len() != self.lights.len();
        for l in self.lights.drain(scene.len().min(self.lights.len())..) {
            self.shadowed -= usize::from(l.shadows());
        }
        // New lights are among `changed`.
        self.lights.resize(scene.len(), GpuLight::zeroed());
        let mut written = Vec::new();
        for &i in changed.iter().filter(|&&i| i < scene.len()) {
            let new = GpuLight::of(&scene[i]);
            let old = self.lights[i];
            if old == new {
                continue;
            }
            self.shadowed = self.shadowed - usize::from(old.shadows()) + usize::from(new.shadows());
            self.lights[i] = new;
            self.dirty
                .lights
                .extend([lit_voxels(&old), lit_voxels(&new)]);
            rehash |= new.position[3] > old.position[3]
                || (self.hashed_at.get(i)).is_none_or(|at| new.at().distance(*at) > SLACK);
            written.push(i);
        }
        if rehash {
            self.rehash(device);
            return;
        }
        if written.is_empty() {
            return;
        }
        profiling::scope!("GpuLights::update");
        self.shadows = self.shadowed > 0;
        let size = std::mem::size_of::<GpuLight>();
        for i in written {
            let bytes = bytemuck::bytes_of(&self.lights[i]);
            queue.write_buffer(&self.light_buffer, (i * size) as u64, bytes);
        }
    }

    /// Build the hash and the buffers again for the lights as they are.
    fn rehash(&mut self, device: &wgpu::Device) {
        profiling::scope!("GpuLights::rehash");
        self.dirty.all = true;
        self.shadows = self.shadowed > 0;
        self.hash = LightHash::build(&self.lights);
        self.hashed_at = self.lights.iter().map(GpuLight::at).collect();
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
            [&self.voxels, &self.sun_voxels],
        );
    }

    /// The tiles the last frame's fill filled, over every layer.
    #[cfg(test)]
    pub(crate) fn filled_tiles(&self) -> u32 {
        self.filled
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

    /// Forget the parts blocking light (the scene is replaced).
    pub(crate) fn clear_blockers(&mut self) {
        self.blockers.clear();
    }

    /// Take the parts at `changed` (indices into `scene.parts`) as they
    /// block light now. Only noted while nothing reads the blockers (the
    /// sky lights nothing, the sun shadows through the shadow map and no
    /// light has `Shadows`), and taken once something does.
    pub(crate) fn update_blockers(&mut self, scene: &Scene, changed: &[usize]) {
        let l = &scene.lighting;
        let used = l.environment_diffuse > 0.0 || l.sun_in_voxels() || self.shadows;
        self.blockers.update(scene, changed, used);
    }

    /// Fill the voxels again where they changed: whole if the lights were
    /// hashed again, the volume's center drifted [`RECENTER`] from where
    /// the camera puts it, the sun the voxels shadow (`sun`, towards it)
    /// came, went or turned past [`SUN_TURN`], or the sky's light (`sky`)
    /// came or went; otherwise only the tiles that lights that changed
    /// reach, and, when the place lights with the sky, the sun shadows
    /// here or a light has `Shadows`, those the parts blocking light that
    /// changed reach ([`GpuLights::reached`]).
    pub(crate) fn fill(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: &Camera,
        sky: bool,
        sun: Option<Vec3>,
    ) {
        let size = IVec3::new(VOLUME[0] as i32, VOLUME[1] as i32, VOLUME[2] as i32);
        let dims = size.as_vec3();
        let half = dims * (VOXEL * 0.5);
        let forward = camera.forward();
        let center = camera.focus() + Vec3::new(forward.x, 0.0, forward.z) * AHEAD;
        let centered = self
            .origin
            .is_some_and(|o| (o + half - center).abs().max_element() <= RECENTER);
        let solid = sky || sun.is_some() || self.shadows;
        let blocked = solid && (self.blockers.due() || !self.marked);
        let sun_kept = match (sun, self.sun) {
            (Some(a), Some(b)) => a.dot(b) >= SUN_TURN,
            (a, b) => a.is_none() && b.is_none(),
        };
        self.filled = 0;
        if self.dirty.is_empty() && centered && !blocked && sky == self.sky && sun_kept {
            return;
        }
        profiling::scope!("GpuLights::fill");
        // Voxel k is centered at 4k + 2.
        let kept = self.origin.map(|o| (o / VOXEL).floor().as_ivec3());
        let mut low = match kept {
            Some(low) if centered => low,
            _ => ((center - half) / VOXEL).floor().as_ivec3(),
        };
        // The fills go up only while the sun reads them, so a sun that
        // just came takes them afresh.
        let sun_came = sun.is_some() && self.sun.is_none();
        let mut touched = Touched::default();
        let mut roofs = Vec::new();
        if solid && (blocked || !centered || sun_came) {
            match self.blockers.mark(low, size) {
                Some(marked) => {
                    if sky {
                        roofs = roofs_of(marked.layers, low);
                    }
                    for run in marked.slot_runs() {
                        let words = run.start / 32..run.end.div_ceil(32);
                        let at = (words.start * 4) as u64;
                        let bits = &marked.bits[words];
                        queue.write_buffer(&self.sky_buffers.solid, at, bytemuck::cast_slice(bits));
                        if sun.is_some() {
                            let words = run.start / 4..run.end.div_ceil(4);
                            let at = (words.start * 4) as u64;
                            let fills = &marked.fills[words];
                            queue.write_buffer(
                                &self.sky_buffers.fills,
                                at,
                                bytemuck::cast_slice(fills),
                            );
                        }
                    }
                    touched = marked.touched;
                }
                // The voxels it moved onto aren't marked yet: it stays
                // where it was, the blockers there still on the GPU, and
                // is filled there only if something else changed.
                None => match kept {
                    Some(kept) if !self.dirty.is_empty() || sky != self.sky || !sun_kept => {
                        low = kept;
                    }
                    _ => return,
                },
            }
        }
        let origin = low.as_vec3() * VOXEL + VOXEL * 0.5;
        let ring = low.rem_euclid(size);
        let whole = self.dirty.all
            || kept.is_none()
            || sky != self.sky
            || !sun_kept
            || solid != self.marked
            || matches!(touched, Touched::All);
        let dirty = std::mem::take(&mut self.dirty);
        self.marked = solid;
        self.origin = Some(origin);
        self.sky = sky;
        self.sun = sun;
        let lit = !self.lights.is_empty();
        queue.write_buffer(
            &self.volume,
            0,
            bytemuck::bytes_of(&VolumeUniform {
                origin: [origin.x, origin.y, origin.z, f32::from(u8::from(lit))],
                size: [dims.x, dims.y, dims.z, f32::from(u8::from(sky))],
                sun: [f32::from(u8::from(sun.is_some())), 0.0, 0.0, 0.0],
                ring: ring.as_vec3().extend(0.0).to_array(),
            }),
        );
        if !lit && !sky && sun.is_none() {
            return;
        }
        let tiles = if whole {
            Tiles::all()
        } else {
            let mut tiles = Tiles::none();
            for span in dirty.lights {
                tiles.mark((span.0 - low, span.1 - low));
            }
            let reach = Reach {
                sky,
                sun,
                shadowed: self.shadowed_reach(low),
                roofs,
            };
            if let Touched::Spans(spans) = touched {
                for span in by_column(&spans, low) {
                    reach.mark(&mut tiles, span);
                }
            }
            // A volume that moved fills what it came onto, and, as for
            // blockers that changed, what that and what it left reach:
            // those it kept saw open sky and nothing in the way past its
            // old faces.
            if let Some(old) = kept.filter(|&old| old != low) {
                let (came, left) = came_and_left(old, low, size);
                for &span in &came {
                    tiles.mark(span);
                }
                if solid {
                    for span in came.into_iter().chain(left) {
                        reach.mark(&mut tiles, span);
                    }
                }
            }
            tiles
        };
        let mut layers = vec![LayerUniform::zeroed(); VOLUME[1] as usize];
        let mut dispatches = Vec::new();
        for y in (0..VOLUME[1]).rev() {
            if let Some((bits, lo, hi)) = tiles.layer(y) {
                layers[y as usize] = LayerUniform {
                    at: [y, lo[0], lo[1], 0],
                    tiles: *bits,
                };
                let across = |axis: usize| (hi[axis] - lo[axis] + 1) * TILE / 8;
                dispatches.push((y, across(0), across(1)));
            }
        }
        self.filled = tiles.count();
        if dispatches.is_empty() {
            return;
        }
        let mut bytes = vec![0u8; (LAYER_STRIDE * u64::from(VOLUME[1])) as usize];
        for (y, layer) in layers.iter().enumerate() {
            let at = y * LAYER_STRIDE as usize;
            bytes[at..at + LAYER_SIZE as usize].copy_from_slice(bytemuck::bytes_of(layer));
        }
        queue.write_buffer(&self.sky_buffers.layers, 0, &bytes);
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
                ring: ring.as_uvec3().extend(0).to_array(),
                flags: [
                    u32::from(lit),
                    u32::from(sky),
                    u32::from(solid),
                    u32::from(sun.is_some()),
                ],
                sun: sun.unwrap_or(Vec3::Y).extend(0.0).to_array(),
            }),
        );
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rook light volume fill"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.fill);
        // One layer at a time from the top: each takes the sky's light
        // from the one above.
        for (y, x, z) in dispatches {
            let offset = (LAYER_STRIDE * u64::from(y)) as u32;
            pass.set_bind_group(0, &self.fill_group, &[offset]);
            pass.dispatch_workgroups(x, z, 1);
        }
    }

    /// For each light with `Shadows`, the voxels a part must reach to
    /// cast its shadows (a ray runs from a voxel the light lights to the
    /// light, its eight neighbours up to a voxel and a half aside), and
    /// those it lights, in the volume starting at voxel `low`.
    fn shadowed_reach(&self, low: IVec3) -> Vec<(Span, Span)> {
        if !self.shadows {
            return Vec::new();
        }
        let margin = Vec3::splat(2.0 * VOXEL);
        let local = |(a, b): Span| (a - low, b - low);
        self.lights
            .iter()
            .filter(|l| l.shadows())
            .map(|light| {
                let (near, far) = light.lit_box();
                (
                    local(voxels_of(near - margin, far + margin)),
                    local(lit_voxels(light)),
                )
            })
            .collect()
    }
}

/// The blocker changes in `spans` (the world's voxels) as boxes of the
/// volume starting at voxel `low`, one per tile column (or run of them)
/// with the changes' heights there joined: they reach the same tiles but
/// for their heights, so a crowd of small parts moving costs a few boxes.
fn by_column(spans: &[Span], low: IVec3) -> Vec<Span> {
    let tile = TILE as i32;
    let mut columns: FxHashMap<(IVec2, IVec2), (i32, i32)> = FxHashMap::default();
    for &(lo, hi) in spans {
        let (lo, hi) = (lo - low, hi - low);
        let column = |v: IVec3| IVec2::new(v.x, v.z).div_euclid(IVec2::splat(tile));
        let key = (column(lo), column(hi));
        let heights = columns.entry(key).or_insert((lo.y, hi.y));
        *heights = (heights.0.min(lo.y), heights.1.max(hi.y));
    }
    // In any order: marking tiles is a union.
    columns
        .into_iter()
        .map(|((a, b), (y0, y1))| {
            (
                IVec3::new(a.x * tile, y0, a.y * tile),
                IVec3::new(b.x * tile + tile - 1, y1, b.y * tile + tile - 1),
            )
        })
        .collect()
}

/// What a blocker change reaches beyond its own voxels.
struct Reach {
    /// The place lights with the sky.
    sky: bool,
    /// The sun the voxels shadow (towards it).
    sun: Option<Vec3>,
    /// [`GpuLights::shadowed_reach`].
    shadowed: Vec<(Span, Span)>,
    /// With the sky's light, [`roofs_of`] the volume.
    roofs: Vec<u64>,
}

impl Reach {
    /// Mark the tiles whose voxels a blocker change in `span` (the
    /// volume's voxels) may light differently: with the sky's light, the
    /// span's columns down to the bottom (whether a voxel there is open to
    /// the sky may change), and in every layer below the span, two voxels
    /// wider each way per layer down (`sky_at` in
    /// `shaders/light_volume.wgsl` takes the five by five above), the
    /// tiles with a blocked voxel above that layer: only a voxel with one
    /// above it takes its neighbors' light, the rest are open to all of
    /// the sky whatever changed; with the sun, every voxel whose way to
    /// the sun crosses the span (`sunlit`); and the voxels each light
    /// with `Shadows` whose rays may cross it lights (`unblocked`).
    fn mark(&self, tiles: &mut Tiles, (lo, hi): Span) {
        if self.sky {
            tiles.mark((lo.with_y(0), hi));
            for y in 0..hi.y {
                let wider = IVec3::new(2, 0, 2) * (hi.y - y);
                // Without the blockers' layers, every tile may be.
                let roofed = |t: usize| self.roofs.get(t).is_none_or(|r| r >> (y + 1) != 0);
                tiles.mark_where((lo.with_y(y) - wider, hi.with_y(y) + wider), roofed);
            }
        }
        if let Some(d) = self.sun {
            // The span's voxels as a box (voxel k from k to k + 1), swept
            // away from the sun until all of it leaves the volume.
            let (from, to) = (lo.as_vec3(), (hi + 1).as_vec3());
            let n = Vec3::new(VOLUME[0] as f32, VOLUME[1] as f32, VOLUME[2] as f32);
            let mut t = f32::INFINITY;
            for axis in 0..3 {
                if d[axis] < -1e-6 {
                    t = t.min((n[axis] - from[axis]) / -d[axis]);
                } else if d[axis] > 1e-6 {
                    t = t.min(to[axis] / d[axis]);
                }
            }
            let away = -d * t;
            let (a, b) = (from.min(from + away), to.max(to + away));
            let (a, b) = (a.floor().as_ivec3() - 1, b.ceil().as_ivec3());
            // The sun is above (it lights the voxels only then), so the
            // box sweeps down: each layer takes only the part of the
            // sweep crossing it (a voxel of margin around, as the whole
            // sweep's box has), not the box around all of it, which for a
            // low sun is mostly voxels the sweep never reaches.
            if d.y < 1e-3 {
                tiles.mark((a, b));
            } else {
                for y in a.y..=b.y {
                    // When the box overlaps the layer and the voxel either
                    // side of it.
                    let (y0, y1) = ((y - 1) as f32, (y + 2) as f32);
                    let s0 = ((from.y - y1) / d.y).max(0.0);
                    let s1 = ((to.y - y0) / d.y).min(t);
                    if s0 > s1 {
                        continue;
                    }
                    let (f0, f1) = (from - d * s0, from - d * s1);
                    let (t0, t1) = (to - d * s0, to - d * s1);
                    let (lo, hi) = (
                        f0.min(f1).floor().as_ivec3() - 1,
                        t0.max(t1).ceil().as_ivec3(),
                    );
                    tiles.mark((lo.with_y(y), hi.with_y(y)));
                }
            }
        }
        for &((a, b), lit) in &self.shadowed {
            if a.cmple(hi).all() && b.cmpge(lo).all() {
                tiles.mark(lit);
            }
        }
    }
}

/// The voxels a volume moving from voxel `old` to `new` (its lowest)
/// came onto and those it left, in the moved volume's own voxels.
fn came_and_left(old: IVec3, new: IVec3, size: IVec3) -> (Vec<Span>, Vec<Span>) {
    let (before, after) = ((old, old + size - 1), (new, new + size - 1));
    let local = |(lo, hi): Span| (lo - new, hi - new);
    let came = blockers::minus(after, Some(before)).into_iter().map(local);
    let left = blockers::minus(before, Some(after)).into_iter().map(local);
    (came.collect(), left.collect())
}

/// Each tile's blocked layers (bit `y`: some voxel of the tile's layer
/// `y` is blocked), in the volume starting at voxel `low`, from the
/// blockers' layers by column ([`blockers::Marked::layers`], kept at slots
/// anchored to the world).
fn roofs_of(layers: &[u64], low: IVec3) -> Vec<u64> {
    let [nx, ny, nz] = VOLUME.map(|n| n as i32);
    // A column's layers rotate into the volume's own like its voxels.
    const _: () = assert!(VOLUME[1] == u64::BITS);
    let ring = low.rem_euclid(IVec3::new(nx, ny, nz));
    let tile = TILE as i32;
    let mut roofs = vec![0u64; (ACROSS[0] * ACROSS[1]) as usize];
    for z in 0..nz {
        let row = nx * ((z + ring.z) % nz);
        let tiles = ACROSS[0] as i32 * (z / tile);
        for x in 0..nx {
            let t = (tiles + x / tile) as usize;
            roofs[t] |= layers[(row + (x + ring.x) % nx) as usize];
        }
    }
    for roof in &mut roofs {
        *roof = roof.rotate_right(ring.y as u32);
    }
    roofs
}

/// The voxels a light lights (the world's).
fn lit_voxels(light: &GpuLight) -> Span {
    let (near, far) = light.lit_box();
    voxels_of(near, far)
}

/// The voxels whose centers may lie from `min` to `max` (the world's).
fn voxels_of(min: Vec3, max: Vec3) -> Span {
    (
        (min / VOXEL).floor().as_ivec3(),
        (max / VOXEL).floor().as_ivec3(),
    )
}

/// The light group's entries: where the voxels are, the light and sun
/// voxels, and how they're sampled.
fn group_entries<'a>(
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

/// The fill pass's bind group; `voxels` are the light voxels and the sun
/// voxels.
fn fill_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffers: [&wgpu::Buffer; 4],
    sky: &SkyBuffers,
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
    for (binding, buffer) in [(6, &sky.solid), (7, &sky.skies), (9, &sky.fills)] {
        entries.push(wgpu::BindGroupEntry {
            binding,
            resource: buffer.as_entire_binding(),
        });
    }
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
