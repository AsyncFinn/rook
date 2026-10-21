//! The glyph atlas: one 8-bit texture of coverage masks, packed in shelves
//! and kept by mask key. When it fills, it starts again empty.

use rustc_hash::FxHashMap;

use rook_types::GlyphMask;

/// Width and height, in texels.
pub(super) const SIZE: u32 = 2048;
/// Empty texels around each mask, so filtering never reads a neighbour.
const GAP: u32 = 1;

#[derive(Debug)]
pub(super) struct Atlas {
    pub(super) texture: wgpu::Texture,
    /// Each mask's rectangle (left, top, right, bottom) in texels.
    placed: FxHashMap<u64, [f32; 4]>,
    /// The shelf being filled: its top, height, and how far along it is.
    shelf: (u32, u32, u32),
    /// Counts the times it started again.
    pub(super) resets: u64,
}

impl Atlas {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rook gui glyphs"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self {
            texture,
            placed: FxHashMap::default(),
            shelf: (0, 0, 0),
            resets: 0,
        }
    }

    /// Where `mask` is, uploading it if it's new; `None` if it can't fit
    /// even in an empty atlas.
    pub(super) fn place(&mut self, queue: &wgpu::Queue, mask: &GlyphMask) -> Option<[f32; 4]> {
        if let Some(&r) = self.placed.get(&mask.key) {
            return Some(r);
        }
        let (w, h) = (mask.width + GAP, mask.height + GAP);
        if w > SIZE || h > SIZE {
            return None;
        }
        let (mut top, mut height, mut x) = self.shelf;
        if x + w > SIZE {
            top += height;
            height = 0;
            x = 0;
        }
        if top + h > SIZE {
            // Full: start again.
            self.placed.clear();
            self.resets += 1;
            (top, height, x) = (0, 0, 0);
        }
        self.shelf = (top, height.max(h), x + w);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y: top, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &mask.coverage,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mask.width),
                rows_per_image: Some(mask.height),
            },
            wgpu::Extent3d {
                width: mask.width,
                height: mask.height,
                depth_or_array_layers: 1,
            },
        );
        let r = [x, top, x + mask.width, top + mask.height].map(|v| v as f32);
        self.placed.insert(mask.key, r);
        Some(r)
    }
}
