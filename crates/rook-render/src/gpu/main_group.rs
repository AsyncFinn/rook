//! The main pass's group 1: the shadow map, the surface pattern and water arrays, and the sky's reflection.

use super::*;

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

/// The layout of group 1: the shadow map, the surface pattern and water
/// arrays, the sky's reflection cube, the table its reflection goes by and
/// the skybox's sampler.
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
            texture_entry(5, wgpu::TextureViewDimension::Cube),
            texture_entry(6, wgpu::TextureViewDimension::D2),
            wgpu::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// The main pass's group 1: the shadow map, the surface patterns (linear:
/// gray 0.5 is a factor of 1 after doubling), water's normal frames, the
/// sky's reflection cube, the BRDF table and the skybox's sampler.
pub(crate) fn main_bind_group(
    cx: &UploadContext<'_>,
    layout: &wgpu::BindGroupLayout,
    [shadow_sampler, sky_sampler]: [&wgpu::Sampler; 2],
    [shadow_view, surfaces, water, sky_cube, brdf]: [&wgpu::TextureView; 5],
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
            wgpu::BindGroupEntry {
                binding: 5,
                resource: view(sky_cube),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: view(brdf),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(sky_sampler),
            },
        ],
    })
}
