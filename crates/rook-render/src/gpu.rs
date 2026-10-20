//! GPU-side data: the uniform globals, per-instance data, meshes and images.

pub(crate) mod cache;
pub(crate) mod pool;

use rook_content::decode::image::Encoding;

use super::*;
use crate::effects::particles::Rng;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Globals {
    pub(crate) view_proj: [[f32; 4]; 4],
    pub(crate) camera_pos: [f32; 4],
    pub(crate) sun_dir: [f32; 4],
    /// `Lighting::sun_color`, in light units.
    pub(crate) sun_color: [f32; 4],
    pub(crate) ambient: [f32; 4],
    pub(crate) outdoor_ambient: [f32; 4],
    pub(crate) fog_color: [f32; 4],
    pub(crate) fog_range: [f32; 4],
    pub(crate) shadow_view_proj: [[f32; 4]; 4],
    /// A texel in uv, the normal offset, the depth bias, and how far
    /// shadows reach (0 for none).
    pub(crate) shadow: [f32; 4],
    /// Each face's image right and up (`NormalId::image_axes`), in
    /// `NormalId` order; w holds the studs per surface pattern tile.
    pub(crate) faces: [[f32; 4]; 12],
    /// What reflective parts take on (`Lighting::reflection`, sRGB); w is
    /// `Lighting::environment_specular`.
    pub(crate) sky: [f32; 4],
    /// From clip space to directions from the camera, for the skybox.
    pub(crate) sky_from_clip: [[f32; 4]; 4],
    /// Each skybox image's direction, right and up ([`SKYBOX_FACES`]).
    pub(crate) sky_faces: [[f32; 4]; 18],
    /// Seconds on the renderer's clock, the waves' size and speed, and how
    /// far grass is drawn ([`TerrainLook`]).
    pub(crate) terrain: [f32; 4],
    /// `TerrainLook::wind`; w is water's phase (`Renderer::water_phase`).
    pub(crate) wind: [f32; 4],
    /// The sky's colour overhead, at the horizon and below
    /// (`Lighting::sky_top`, `sky_horizon`, `sky_bottom`, sRGB): water
    /// reflects the first two, and all three light surfaces. `sky_top`'s w
    /// is `Lighting::environment_diffuse`.
    pub(crate) sky_top: [f32; 4],
    pub(crate) sky_horizon: [f32; 4],
    pub(crate) sky_bottom: [f32; 4],
    /// Each skybox image's slot in the image table, when there is one.
    pub(crate) sky_images: [[u32; 4]; 2],
    /// Each terrain layer's blend ([`TerrainLayer`]), two per layer: the
    /// height's multiplier and offset, the edge's softness and the studs
    /// per repeat; then its height map's slot in the image table, whether
    /// it's laid like a box, and its [`Detile`].
    pub(crate) terrain_layers: [[f32; 4]; 2 * TERRAIN_LAYERS],
}

/// Instance flags (see `shaders/common.wgsl`).
pub(crate) const CAST: u32 = 1;
pub(crate) const NEON: u32 = 2;
pub(crate) const MODE_FACE: u32 = 1;
pub(crate) const MODE_TILED: u32 = 2;
pub(crate) const MODE_MESH_UV: u32 = 3;
/// A part's material textures' [`TilePattern`], in the bits an image
/// layer's mode takes.
pub(crate) const PATTERN_SHIFT: u32 = 2;
pub(crate) const UPRIGHT: u32 = 1 << 7;
/// A base part drawn in its own colour without light (an image layer's
/// face takes this bit).
pub(crate) const UNLIT: u32 = 1 << 4;
/// Where the surface patterns start: 3 bits per face.
pub(crate) const SURFACES_SHIFT: u32 = 8;
/// The part has material textures, tiled at `tile.xy` studs.
pub(crate) const MAPS: u32 = 1 << 26;
/// Glass and ForceField draw their own way (`shaders/parts.wgsl`).
pub(crate) const GLASS: u32 = 1 << 27;
pub(crate) const FORCE_FIELD: u32 = 1 << 28;
/// A wedge or corner wedge, whose slopes take material textures in their
/// own plane.
pub(crate) const SLOPED: u32 = 1 << 29;
/// The material textures go through the mesh's UVs (a SurfaceAppearance),
/// with their alpha mode in the surfaces' bits (a MeshPart has none).
pub(crate) const MAPS_UV: u32 = 1 << 30;
/// The mesh's vertex colours replace the part's colour (a union with
/// `UsePartColor` off).
pub(crate) const VERTEX_COLORS: u32 = 1 << 31;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Instance {
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
    /// A Texture's studs per tile and offset; for a part with material
    /// textures, their studs per tile, and on terrain their [`Detile`].
    tile: [f32; 4],
    /// CAST, NEON, an image layer's mode or a part's tile pattern (bits
    /// 2-3), an image layer's face (4-6),
    /// UPRIGHT (bit 7), from bit 8 each face's surface pattern + 1, three
    /// bits per face, and the bits from 26 (wgpu allows 16 vertex
    /// attributes in all).
    flags: u32,
    /// Its images' slots in the image table, 16 bits each, in
    /// [`renderer::ImageSet`] order (unused without a table).
    images: [u32; 3],
    /// A part's `Reflectance`, roughness and metalness; w is the blue of
    /// a SurfaceAppearance's tint, whose red and green are in `tile.zw`.
    material: [f32; 4],
}

impl Instance {
    pub(crate) fn new(
        p: &PartInstance,
        (box_min, box_max): (Vec3, Vec3),
        images: [u32; 3],
    ) -> Self {
        let mut flags = 0;
        let mut tile = [1.0, 1.0, 0.0, 0.0];
        let mut material = [0.0; 4];
        if p.upright {
            flags |= UPRIGHT;
        }
        match p.layer {
            Layer::Base => {
                if p.cast_shadow {
                    flags |= CAST;
                }
                if p.vertex_colors {
                    flags |= VERTEX_COLORS;
                }
                if p.unlit {
                    flags |= UNLIT;
                }
                flags |= match p.material {
                    Material::Neon => NEON,
                    Material::Glass => GLASS,
                    Material::ForceField => FORCE_FIELD,
                    _ => 0,
                };
                // A map gives the value itself; without one, the material's.
                let shading = p.material.shading(p.material_set);
                let maps = p.maps;
                material = [
                    p.reflectance.clamp(0.0, 1.0),
                    if maps.roughness.is_some() {
                        1.0
                    } else {
                        shading.roughness
                    },
                    if maps.metalness.is_some() {
                        1.0
                    } else {
                        shading.metalness
                    },
                    0.0,
                ];
                if maps.on_mesh_uv {
                    flags |= MAPS | MAPS_UV | (maps.alpha as u32) << SURFACES_SHIFT;
                    let t = maps.tint;
                    tile = [1.0, 1.0, t[0], t[1]];
                    material[3] = t[2];
                } else if maps.any() {
                    // Only terrain's maps say how their alpha is used, in
                    // the bits Plastic's surfaces use otherwise.
                    flags |= MAPS | (maps.alpha as u32) << SURFACES_SHIFT;
                    if matches!(
                        p.shape,
                        DrawShape::Part(PartShape::Wedge | PartShape::CornerWedge)
                    ) {
                        flags |= SLOPED;
                    }
                    flags |= (maps.pattern as u32) << PATTERN_SHIFT;
                    let [z, w] = if maps.seed != 0 {
                        // Studs the part's textures start in by.
                        let mut rng = Rng::new(maps.seed);
                        (glam::Vec2::new(rng.next(), rng.next()) * maps.studs_per_tile).to_array()
                    } else {
                        [maps.detile.shift, maps.detile.turn]
                    };
                    tile = [maps.studs_per_tile.x, maps.studs_per_tile.y, z, w];
                }
                if p.material.shows_surfaces() {
                    for (face, pattern) in p.surfaces.iter().enumerate() {
                        if let Some(pattern) = pattern {
                            flags |= (*pattern as u32 + 1) << (SURFACES_SHIFT + face as u32 * 3);
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
            images,
            material,
        }
    }

    /// This instance where `part` is now (its look unchanged).
    pub(crate) fn placed(mut self, part: &PartInstance) -> Self {
        self.position = part.position.to_array();
        self.right = part.rotation.x_axis.to_array();
        self.up = part.rotation.y_axis.to_array();
        self.back = part.rotation.z_axis.to_array();
        self
    }

    /// The box around what this instance draws in the world, before the
    /// shader moves any vertex. A free slot (all zeroes) draws nothing.
    pub(crate) fn world_box(&self) -> Aabb {
        let size = Vec3::from(self.size);
        if size == Vec3::ZERO {
            return Aabb::EMPTY;
        }
        let (lo, hi) = (
            Vec3::from(self.box_min) * size,
            Vec3::from(self.box_max) * size,
        );
        let rotation = glam::Mat3::from_cols(self.right.into(), self.up.into(), self.back.into());
        let centre = Vec3::from(self.position) + rotation * ((lo + hi) * 0.5);
        let abs = glam::Mat3::from_cols(
            rotation.x_axis.abs(),
            rotation.y_axis.abs(),
            rotation.z_axis.abs(),
        );
        let half = abs * ((hi - lo) * 0.5).abs();
        Aabb {
            min: centre - half,
            max: centre + half,
        }
    }
}

impl Upload for Mesh {
    type Gpu = GpuMesh;
    type Context<'a> = MeshPool;

    fn upload_size(&self) -> usize {
        self.positions.len() * std::mem::size_of::<mesh::Vertex>() + self.indices.len() * 4
    }

    fn upload(&self, pool: &mut MeshPool) -> GpuMesh {
        profiling::scope!("upload mesh");
        pool.add(&mesh::files::from_file(self))
    }
}

/// An image on the GPU, viewed as sRGB colour (decals, colour maps) or as
/// linear data (normal, roughness and metalness maps).
#[derive(Debug)]
pub(crate) struct GpuImage {
    pub(crate) srgb: wgpu::TextureView,
    pub(crate) linear: wgpu::TextureView,
    /// The GPU memory its textures hold.
    pub(crate) bytes: usize,
}

/// Where the device can't sample blocks, block-compressed images (material
/// textures) go up as RGBA8 from their first level this size or smaller:
/// their full 1024 would take four times the memory.
const UNCOMPRESSED_MAX_SIZE: u32 = 512;

impl GpuImage {
    /// A 1 × 1 image of one RGBA8 colour.
    pub(crate) fn solid(cx: &UploadContext<'_>, label: &str, rgba: [u8; 4]) -> Self {
        Self::new(cx, label, 1, 1, Encoding::Rgba8, &[rgba.to_vec()])
    }

    /// `mips` (largest first) of `width` × `height` in `encoding`, viewed
    /// both ways: one texture where the device can view it as sRGB too,
    /// else a second copy stored as sRGB.
    fn new(
        cx: &UploadContext<'_>,
        label: &str,
        width: u32,
        height: u32,
        encoding: Encoding,
        mips: &[Vec<u8>],
    ) -> Self {
        let format = match encoding {
            Encoding::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
            Encoding::Bc1 => wgpu::TextureFormat::Bc1RgbaUnorm,
            Encoding::Bc3 => wgpu::TextureFormat::Bc3RgbaUnorm,
            Encoding::Bc7 => wgpu::TextureFormat::Bc7RgbaUnorm,
        };
        let srgb_format = format.add_srgb_suffix();
        let view = |texture: &wgpu::Texture, format| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(format),
                dimension: Some(wgpu::TextureViewDimension::D2),
                ..Default::default()
            })
        };
        let linear = texture_in(cx, label, format, width, height, &[mips]);
        let mut bytes: usize = mips.iter().map(Vec::len).sum();
        let srgb = if cx.view_formats {
            view(&linear, srgb_format)
        } else {
            bytes *= 2;
            let copy = texture_in(cx, label, srgb_format, width, height, &[mips]);
            view(&copy, srgb_format)
        };
        Self {
            srgb,
            linear: view(&linear, format),
            bytes,
        }
    }
}

impl Upload for Image {
    type Gpu = GpuImage;
    type Context<'a> = UploadContext<'a>;

    fn upload_size(&self) -> usize {
        self.byte_len()
    }

    fn upload(&self, cx: &mut UploadContext<'_>) -> GpuImage {
        profiling::scope!("upload image");
        if self.encoding != Encoding::Rgba8 && !cx.block_compression {
            let rgba = self.decompressed(UNCOMPRESSED_MAX_SIZE);
            return rgba.upload(cx);
        }
        let (w, h) = (self.width, self.height);
        GpuImage::new(cx, "rook image", w, h, self.encoding, &self.mips)
    }
}

/// A texture of `layers`, each a mip chain (largest first) of RGBA8
/// `width` × `height`, stored linear and viewable as sRGB too where the
/// device allows it.
pub(crate) fn texture_with_mips(
    cx: &UploadContext<'_>,
    label: &str,
    width: u32,
    height: u32,
    layers: &[&[Vec<u8>]],
) -> wgpu::Texture {
    texture_in(
        cx,
        label,
        wgpu::TextureFormat::Rgba8Unorm,
        width,
        height,
        layers,
    )
}

/// [`texture_with_mips`] stored as `format` (RGBA8 or blocks), each level
/// rows of pixels or of blocks.
fn texture_in(
    cx: &UploadContext<'_>,
    label: &str,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    layers: &[&[Vec<u8>]],
) -> wgpu::Texture {
    let srgb_view = [format.add_srgb_suffix()];
    let view_formats: &[_] = if cx.view_formats && !format.is_srgb() {
        &srgb_view
    } else {
        &[]
    };
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
        view_formats,
    });
    let (bw, bh) = format.block_dimensions();
    let block_bytes = format.block_copy_size(None).expect("colour format");
    for (layer, levels) in layers.iter().enumerate() {
        for (level, pixels) in levels.iter().enumerate() {
            // A level smaller than a block takes a whole one.
            let w = ((width >> level).max(1)).next_multiple_of(bw);
            let h = ((height >> level).max(1)).next_multiple_of(bh);
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
                    bytes_per_row: Some(w / bw * block_bytes),
                    rows_per_image: Some(h / bh),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    texture
}

/// A draw's images (group 2): its image (a mesh texture, decal or skybox
/// face) and colour map as sRGB, its normal, roughness and metalness maps
/// as linear data.
pub(crate) fn images_bind_group(
    cx: &UploadContext<'_>,
    [image, color, normal, roughness, metalness]: [&GpuImage; 5],
) -> wgpu::BindGroup {
    let views = [
        &image.srgb,
        &color.srgb,
        &normal.linear,
        &roughness.linear,
        &metalness.linear,
    ];
    let entries: Vec<wgpu::BindGroupEntry> = views
        .iter()
        .enumerate()
        .map(|(i, view)| wgpu::BindGroupEntry {
            binding: i as u32,
            resource: wgpu::BindingResource::TextureView(view),
        })
        .collect();
    cx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rook images"),
        layout: cx.image_layout,
        entries: &entries,
    })
}

/// A texture array of square layers, each with its mip chain, as the view
/// group 1 binds (the surface patterns, water's normal frames).
pub(crate) fn layers_view(
    cx: &UploadContext<'_>,
    label: &str,
    size: u32,
    layers: &[Vec<Vec<u8>>],
) -> wgpu::TextureView {
    let layers: Vec<&[Vec<u8>]> = layers.iter().map(Vec::as_slice).collect();
    texture_with_mips(cx, label, size, size, &layers).create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}
/// The sun's shadow map, and its comparison sampler.
pub(crate) fn shadow_map(device: &wgpu::Device) -> (wgpu::TextureView, wgpu::Sampler) {
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
    let view = shadow_texture.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("rook shadow map"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        // Reverse Z: lit when the point is at least as near the sun as
        // what the map holds.
        compare: Some(wgpu::CompareFunction::GreaterEqual),
        ..Default::default()
    });
    (view, sampler)
}

/// The layout of group 1: the shadow map, and the surface pattern and
/// water arrays.
pub(crate) fn main_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
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
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
    })
}

/// The main pass's group 1: the shadow map, the surface patterns (linear:
/// grey 0.5 is a factor of 1 after doubling) and water's normal frames.
pub(crate) fn main_bind_group(
    cx: &UploadContext<'_>,
    layout: &wgpu::BindGroupLayout,
    shadow_view: &wgpu::TextureView,
    shadow_sampler: &wgpu::Sampler,
    surfaces: &wgpu::TextureView,
    water: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let view = wgpu::BindingResource::TextureView;
    cx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rook shadow map, surfaces and water"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: view(shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(shadow_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: view(surfaces),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(cx.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: view(water),
            },
        ],
    })
}
