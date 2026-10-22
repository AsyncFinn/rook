//! The images GUI quads are drawn through: each bound with the sampler its
//! paint asks for (blended or nearest, edge pixels carried on or
//! repeating), groups' textures, and a white one for quads without an
//! image. Quads are drawn in runs that share their image.

use std::ops::Range;

use rook_types::ImagePaint;

use super::super::*;

/// An image as GUI quads sample it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ImageKey {
    File {
        hash: ImageHash,
        /// An index into [`GuiImages`]' samplers: bit 0 nearest, bit 1
        /// repeating.
        sampling: usize,
    },
    /// A group's texture, by its key.
    Group(u64),
}

impl ImageKey {
    pub(crate) fn of(paint: &ImagePaint) -> Self {
        Self::File {
            hash: ImageHash(paint.hash),
            sampling: usize::from(paint.pixelated) | usize::from(paint.tiled) << 1,
        }
    }
}

/// Quads drawn with one image bound (`None`: the white one, for fills and
/// glyphs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Run {
    pub(crate) quads: Range<u32>,
    pub(crate) image: Option<ImageKey>,
}

/// The runs of a list of quads, given each quad's image.
pub(crate) fn runs(images: impl IntoIterator<Item = Option<ImageKey>>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for (i, image) in images.into_iter().enumerate() {
        let i = i as u32;
        match out.last_mut() {
            Some(run) if run.image == image => run.quads.end = i + 1,
            _ => out.push(Run {
                quads: i..i + 1,
                image,
            }),
        }
    }
    out
}

#[derive(Debug)]
pub(crate) struct GuiImages {
    samplers: [wgpu::Sampler; 4],
    white: wgpu::TextureView,
    /// Group 1 of the screen pass: an image and its sampler.
    pub(crate) layout: wgpu::BindGroupLayout,
    groups: HashMap<Option<ImageKey>, wgpu::BindGroup>,
    /// The groups' textures now drawn, by group key.
    group_views: HashMap<u64, wgpu::TextureView>,
}

impl GuiImages {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let sampler = |i: usize| {
            let (filter, address) = (
                if i & 1 == 0 {
                    wgpu::FilterMode::Linear
                } else {
                    wgpu::FilterMode::Nearest
                },
                if i & 2 == 0 {
                    wgpu::AddressMode::ClampToEdge
                } else {
                    wgpu::AddressMode::Repeat
                },
            );
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("rook gui images"),
                address_mode_u: address,
                address_mode_v: address,
                mag_filter: filter,
                min_filter: filter,
                mipmap_filter: if i & 1 == 0 {
                    wgpu::MipmapFilterMode::Linear
                } else {
                    wgpu::MipmapFilterMode::Nearest
                },
                // A slice squeezed along one axis (a style's middle row,
                // 26 texels in 2 pixels) would otherwise take a mip
                // blurred along both and smear its edge sideways, which
                // Studio's doesn't.
                anisotropy_clamp: if i & 1 == 0 { 16 } else { 1 },
                ..Default::default()
            })
        };
        let white = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("rook gui white"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[255; 4],
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook gui image"),
            entries: &image_entries(0),
        });
        Self {
            samplers: std::array::from_fn(sampler),
            white: white.create_view(&Default::default()),
            layout,
            groups: HashMap::default(),
            group_views: HashMap::default(),
        }
    }

    /// The groups' textures from now on, by group key; bind groups of
    /// earlier ones go.
    pub(crate) fn set_groups(&mut self, views: HashMap<u64, wgpu::TextureView>) {
        self.groups
            .retain(|key, _| !matches!(key, Some(ImageKey::Group(_))));
        self.group_views = views;
    }

    /// The view and sampler `key` draws with, once its image is on the GPU.
    pub(crate) fn view<'a>(
        &'a self,
        key: Option<ImageKey>,
        cache: &'a GpuCache<ImageHash, Image>,
    ) -> Option<(&'a wgpu::TextureView, &'a wgpu::Sampler)> {
        match key {
            None => Some((&self.white, &self.samplers[0])),
            // Raw sRGB values: GUI blends in sRGB.
            Some(ImageKey::File { hash, sampling }) => cache
                .get(&hash)
                .map(|gpu| (&gpu.linear, &self.samplers[sampling])),
            Some(ImageKey::Group(key)) => {
                (self.group_views.get(&key)).map(|view| (view, &self.samplers[0]))
            }
        }
    }

    /// Make the screen's bind groups of `runs` whose images are up.
    pub(crate) fn bind(
        &mut self,
        device: &wgpu::Device,
        runs: &[Run],
        cache: &GpuCache<ImageHash, Image>,
    ) {
        for run in runs {
            if self.groups.contains_key(&run.image) {
                continue;
            }
            let Some((view, sampler)) = self.view(run.image, cache) else {
                continue;
            };
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rook gui image"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            });
            self.groups.insert(run.image, group);
        }
    }

    /// The screen's bind group of `key`, if its image is up.
    pub(crate) fn group(&self, key: Option<ImageKey>) -> Option<&wgpu::BindGroup> {
        self.groups.get(&key)
    }
}

/// An image and its sampler, at `first` and the binding after it.
pub(crate) fn image_entries(first: u32) -> [wgpu::BindGroupLayoutEntry; 2] {
    [
        wgpu::BindGroupLayoutEntry {
            binding: first,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: first + 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}
