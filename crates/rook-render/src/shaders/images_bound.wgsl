// The draw's images, bound per draw (white, or a flat normal, when it has
// none): its image (a mesh texture, a decal or a skybox face), and its
// material's colour, normal, roughness (green) and metalness (red) maps.
// For adapters without binding arrays (`images_table.wgsl`); the
// instance's slots go unused.
@group(2) @binding(0) var image: texture_2d<f32>;
@group(2) @binding(1) var color_map: texture_2d<f32>;
@group(2) @binding(2) var normal_map: texture_2d<f32>;
@group(2) @binding(3) var roughness_map: texture_2d<f32>;
@group(2) @binding(4) var metalness_map: texture_2d<f32>;

// The draw's image, with the derivatives of `uv`.
fn sample_image(images: vec3<u32>, uv: vec2<f32>) -> vec4<f32> {
    return textureSample(image, image_sampler, uv);
}

fn image_size(images: vec3<u32>) -> vec2<u32> {
    return textureDimensions(image);
}

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

// A skybox face's image: bound for its draw.
fn sample_sky(face: u32, uv: vec2<f32>) -> vec4<f32> {
    return textureSample(image, image_sampler, uv);
}

fn sky_size(face: u32) -> vec2<u32> {
    return textureDimensions(image);
}
