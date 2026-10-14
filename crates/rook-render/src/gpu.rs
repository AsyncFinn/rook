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
    /// A Texture's studs per tile and offset.
    tile: [f32; 4],
    /// CAST, NEON, an image layer's mode (bits 2-3) and face (4-6),
    /// UPRIGHT (bit 7), and from bit 8 each face's surface pattern + 1,
    /// three bits per face (wgpu allows 16 vertex attributes in all).
    flags: u32,
    /// A part's `Reflectance`, roughness and metalness.
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
                if p.material == Material::Neon {
                    flags |= NEON;
                }
                let shading = p.material.shading();
                material = [
                    p.reflectance.clamp(0.0, 1.0),
                    shading.roughness,
                    shading.metalness,
                    0.0,
                ];
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

/// An image on the GPU: its bind group (group 2) holds the texture.
#[derive(Debug)]
pub(crate) struct GpuImage {
    pub(crate) bind_group: wgpu::BindGroup,
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
pub(crate) fn texture_with_mips(
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

pub(crate) fn image_bind_group(
    cx: &UploadContext<'_>,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    cx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rook image"),
        layout: cx.image_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(view),
        }],
    })
}

/// The main pass's group 1: the shadow map and the surface patterns.
pub(crate) fn surface_bind_group(
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

pub(crate) fn srgb_to_linear(c: [f32; 3]) -> [f32; 3] {
    c.map(|x| {
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    })
}
