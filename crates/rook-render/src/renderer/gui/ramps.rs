//! Gradients' ramps: one RGBA texture, a ramp a row, kept by ramp key.
//! When it fills, it starts again empty.

use rustc_hash::FxHashMap;

use rook_types::GuiRamp;

/// Rows, so ramps on the screen at once.
pub(super) const ROWS: u32 = 256;

#[derive(Debug)]
pub(super) struct Ramps {
    pub(super) texture: wgpu::Texture,
    /// Each ramp's row.
    placed: FxHashMap<u64, u32>,
    /// Counts the times it started again.
    pub(super) resets: u64,
}

impl Ramps {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rook gui ramps"),
            size: wgpu::Extent3d {
                width: GuiRamp::TEXELS as u32,
                height: ROWS,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self {
            texture,
            placed: FxHashMap::default(),
            resets: 0,
        }
    }

    /// `ramp`'s row, uploading it if it's new.
    pub(super) fn place(&mut self, queue: &wgpu::Queue, ramp: &GuiRamp) -> u32 {
        if let Some(&row) = self.placed.get(&ramp.key) {
            return row;
        }
        if self.placed.len() as u32 >= ROWS {
            self.placed.clear();
            self.resets += 1;
        }
        let row = self.placed.len() as u32;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: row, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&ramp.texels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(GuiRamp::TEXELS as u32 * 4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: GuiRamp::TEXELS as u32,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        self.placed.insert(ramp.key, row);
        row
    }
}
