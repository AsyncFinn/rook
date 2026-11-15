//! Images on the GPU: uploads with their mip chains, sRGB and linear views, and the bind group a draw's images go in.

use rook_content::decode::image::Encoding;

use super::*;

/// An image on the GPU, viewed as sRGB color (decals, color maps) or as
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
    /// A 1 × 1 image of one RGBA8 color.
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
            Encoding::Bc5 => wgpu::TextureFormat::Bc5RgUnorm,
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
    let block_bytes = format.block_copy_size(None).expect("color format");
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
/// face) and color map as sRGB, its normal, roughness, metalness and
/// detail normal maps as linear data.
pub(crate) fn images_bind_group(
    cx: &UploadContext<'_>,
    [image, color, normal, roughness, metalness, detail_normal]: [&GpuImage; 6],
) -> wgpu::BindGroup {
    let views = [
        &image.srgb,
        &color.srgb,
        &normal.linear,
        &roughness.linear,
        &metalness.linear,
        &detail_normal.linear,
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
