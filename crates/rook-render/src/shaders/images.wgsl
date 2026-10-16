// How draws reach their images. With binding arrays (`image_table`),
// every image is in one table, as sRGB and as linear data with the same
// slots, and each instance names its images' slots, 16 bits each, in
// `ImageSet` order (`Instance::images`); slot 0 is white, 1 a flat normal.
// Without them, the draw's images are bound per draw (white, or a flat
// normal, when it has none) and the instance's slots go unused: its image
// (a mesh texture, a decal or a skybox face), and its material's colour,
// normal, roughness (green) and metalness (red) maps.

import package::common::{g, image_sampler};

@if(image_table) enable wgpu_binding_array;

// A draw's images (`ImageSet` order), as `sample_map` takes them.
const IMAGE: u32 = 0u;
const COLOR_MAP: u32 = 1u;
const NORMAL_MAP: u32 = 2u;
const ROUGHNESS_MAP: u32 = 3u;
const METALNESS_MAP: u32 = 4u;

@if(image_table) @group(2) @binding(0) var images_srgb: binding_array<texture_2d<f32>>;
@if(image_table) @group(2) @binding(1) var images_linear: binding_array<texture_2d<f32>>;

@if(!image_table) @group(2) @binding(0) var image: texture_2d<f32>;
@if(!image_table) @group(2) @binding(1) var color_map: texture_2d<f32>;
@if(!image_table) @group(2) @binding(2) var normal_map: texture_2d<f32>;
@if(!image_table) @group(2) @binding(3) var roughness_map: texture_2d<f32>;
@if(!image_table) @group(2) @binding(4) var metalness_map: texture_2d<f32>;

@if(image_table)
fn image_slot(images: vec3<u32>, which: u32) -> u32 {
    return (images[which / 2u] >> ((which & 1u) * 16u)) & 65535u;
}

// The draw's image, with the derivatives of `uv`.
@if(image_table)
fn sample_image(images: vec3<u32>, uv: vec2<f32>) -> vec4<f32> {
    return textureSample(images_srgb[image_slot(images, IMAGE)], image_sampler, uv);
}
@else
fn sample_image(images: vec3<u32>, uv: vec2<f32>) -> vec4<f32> {
    return textureSample(image, image_sampler, uv);
}

@if(image_table)
fn image_size(images: vec3<u32>) -> vec2<u32> {
    return textureDimensions(images_srgb[image_slot(images, IMAGE)]);
}
@else
fn image_size(images: vec3<u32>) -> vec2<u32> {
    return textureDimensions(image);
}

// One of the draw's images: the image and colour map as sRGB, the others
// as linear data.
@if(image_table)
fn sample_map(images: vec3<u32>, which: u32, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let slot = image_slot(images, which);
    if which <= COLOR_MAP {
        return textureSampleGrad(images_srgb[slot], image_sampler, uv, dx, dy);
    }
    return textureSampleGrad(images_linear[slot], image_sampler, uv, dx, dy);
}
@else
fn sample_map(images: vec3<u32>, which: u32, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    switch which {
        case 0u: {
            return textureSampleGrad(image, image_sampler, uv, dx, dy);
        }
        case 1u: {
            return textureSampleGrad(color_map, image_sampler, uv, dx, dy);
        }
        case 2u: {
            return textureSampleGrad(normal_map, image_sampler, uv, dx, dy);
        }
        case 3u: {
            return textureSampleGrad(roughness_map, image_sampler, uv, dx, dy);
        }
        default: {
            return textureSampleGrad(metalness_map, image_sampler, uv, dx, dy);
        }
    }
}

// A terrain layer's height (blue of its roughness-metalness map) at `uv`,
// from its slot in the table. Without a table only the draw's own images
// are bound, so every layer counts as full height and seams blend by
// their corners' weights alone.
@if(image_table)
fn layer_height(slot: u32, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> f32 {
    return textureSampleGrad(images_linear[slot], image_sampler, uv, dx, dy).b;
}
@else
fn layer_height(slot: u32, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> f32 {
    return 1.0;
}

// A skybox face's image: its slot in the table (`Globals::sky_images`),
// or bound for its draw.
@if(image_table)
fn sky_slot(face: u32) -> u32 {
    return g.sky_images[face / 4u][face % 4u];
}

@if(image_table)
fn sample_sky(face: u32, uv: vec2<f32>) -> vec4<f32> {
    return textureSample(images_srgb[sky_slot(face)], image_sampler, uv);
}
@else
fn sample_sky(face: u32, uv: vec2<f32>) -> vec4<f32> {
    return textureSample(image, image_sampler, uv);
}

@if(image_table)
fn sky_size(face: u32) -> vec2<u32> {
    return textureDimensions(images_srgb[sky_slot(face)]);
}
@else
fn sky_size(face: u32) -> vec2<u32> {
    return textureDimensions(image);
}
