// Every image in one table (binding arrays), as sRGB and as linear data
// with the same slots; each instance names its images' slots, 16 bits
// each, in `ImageSet` order (`Instance::images`). Slot 0 is white, 1 a
// flat normal.
@group(2) @binding(0) var images_srgb: binding_array<texture_2d<f32>>;
@group(2) @binding(1) var images_linear: binding_array<texture_2d<f32>>;

fn image_slot(images: vec3<u32>, which: u32) -> u32 {
    return (images[which / 2u] >> ((which & 1u) * 16u)) & 65535u;
}

// The draw's image, with the derivatives of `uv`.
fn sample_image(images: vec3<u32>, uv: vec2<f32>) -> vec4<f32> {
    return textureSample(images_srgb[image_slot(images, IMAGE)], image_sampler, uv);
}

fn image_size(images: vec3<u32>) -> vec2<u32> {
    return textureDimensions(images_srgb[image_slot(images, IMAGE)]);
}

// One of the draw's images: the image and colour map as sRGB, the others
// as linear data.
fn sample_map(images: vec3<u32>, which: u32, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let slot = image_slot(images, which);
    if which <= COLOR_MAP {
        return textureSampleGrad(images_srgb[slot], image_sampler, uv, dx, dy);
    }
    return textureSampleGrad(images_linear[slot], image_sampler, uv, dx, dy);
}

// A skybox face's image (`Globals::sky_images`).
fn sky_slot(face: u32) -> u32 {
    return g.sky_images[face / 4u][face % 4u];
}

fn sample_sky(face: u32, uv: vec2<f32>) -> vec4<f32> {
    return textureSample(images_srgb[sky_slot(face)], image_sampler, uv);
}

fn sky_size(face: u32) -> vec2<u32> {
    return textureDimensions(images_srgb[sky_slot(face)]);
}
