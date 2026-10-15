//! GPU-side data: the uniform globals, per-instance data, meshes and images.

use super::*;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Globals {
    pub(crate) view_proj: [[f32; 4]; 4],
    pub(crate) camera_pos: [f32; 4],
    pub(crate) sun_dir: [f32; 4],
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
    /// The sky's colour overhead and at the horizon (`Lighting::sky_top`,
    /// `Lighting::sky_horizon`, sRGB), which water reflects.
    pub(crate) sky_top: [f32; 4],
    pub(crate) sky_horizon: [f32; 4],
}

/// Instance flags (see `shaders/common.wgsl`).
pub(crate) const CAST: u32 = 1;
pub(crate) const NEON: u32 = 2;
pub(crate) const MODE_FACE: u32 = 1;
pub(crate) const MODE_TILED: u32 = 2;
pub(crate) const MODE_MESH_UV: u32 = 3;
pub(crate) const UPRIGHT: u32 = 1 << 7;
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
    /// textures, their studs per tile.
    tile: [f32; 4],
    /// CAST, NEON, an image layer's mode (bits 2-3) and face (4-6),
    /// UPRIGHT (bit 7), from bit 8 each face's surface pattern + 1, three
    /// bits per face, and the bits from 26 (wgpu allows 16 vertex
    /// attributes in all).
    flags: u32,
    /// A part's `Reflectance`, roughness and metalness; w is the blue of
    /// a SurfaceAppearance's tint, whose red and green are in `tile.zw`.
    material: [f32; 4],
}

impl Instance {
    pub(crate) fn new(p: &PartInstance, (box_min, box_max): (Vec3, Vec3)) -> Self {
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
                    flags |= MAPS;
                    if matches!(
                        p.shape,
                        DrawShape::Part(PartShape::Wedge | PartShape::CornerWedge)
                    ) {
                        flags |= SLOPED;
                    }
                    tile = [maps.studs_per_tile.x, maps.studs_per_tile.y, 0.0, 0.0];
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
            material,
        }
    }
}

#[derive(Debug)]
pub(crate) struct GpuMesh {
    pub(crate) vertices: wgpu::Buffer,
    pub(crate) indices: wgpu::Buffer,
    pub(crate) index_count: u32,
    /// The mesh's bounds in its own units.
    pub(crate) bounds: (Vec3, Vec3),
}

impl GpuMesh {
    pub(crate) fn upload(device: &wgpu::Device, data: &mesh::MeshData) -> Self {
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

/// An image on the GPU, viewed as sRGB colour (decals, colour maps) or as
/// linear data (normal, roughness and metalness maps).
#[derive(Debug)]
pub(crate) struct GpuImage {
    pub(crate) srgb: wgpu::TextureView,
    pub(crate) linear: wgpu::TextureView,
}

impl GpuImage {
    /// A 1 × 1 image of one RGBA8 colour.
    pub(crate) fn solid(cx: &UploadContext<'_>, label: &str, rgba: [u8; 4]) -> Self {
        let texture = texture_with_mips(cx, label, 1, 1, &[&[rgba.to_vec()]]);
        Self::views(&texture)
    }

    fn views(texture: &wgpu::Texture) -> Self {
        let view = |format| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(format),
                dimension: Some(wgpu::TextureViewDimension::D2),
                ..Default::default()
            })
        };
        Self {
            srgb: view(wgpu::TextureFormat::Rgba8UnormSrgb),
            linear: view(wgpu::TextureFormat::Rgba8Unorm),
        }
    }
}

impl Upload for Image {
    type Gpu = GpuImage;

    fn upload_size(&self) -> usize {
        self.byte_len()
    }

    fn upload(&self, cx: &UploadContext<'_>) -> GpuImage {
        profiling::scope!("upload image");
        let texture = texture_with_mips(
            cx,
            "rook image",
            self.width,
            self.height,
            &[self.mips.as_slice()],
        );
        GpuImage::views(&texture)
    }
}

/// A texture of `layers`, each a mip chain (largest first) of RGBA8
/// `width` × `height`, stored linear and viewable as sRGB too.
pub(crate) fn texture_with_mips(
    cx: &UploadContext<'_>,
    label: &str,
    width: u32,
    height: u32,
    layers: &[&[Vec<u8>]],
) -> wgpu::Texture {
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
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
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
