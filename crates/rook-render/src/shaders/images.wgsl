// How draws reach their images. With binding arrays (`image_table`),
// every image is in one table, as sRGB and as linear data with the same
// slots, and each instance names its images' slots, 16 bits each, in
// `ImageSet` order (`Instance::images`); slot 0 is white, 1 a flat normal.
// Without them, the draw's images are bound per draw (white, or a flat
// normal, when it has none) and the instance's slots go unused: its image
// (a mesh texture, a decal or a skybox face), and its material's color,
// normal, roughness (green) and metalness (red) maps.

import package::common::{g, image_sampler, Taps, turned};
import package::tone::srgb_to_linear;

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

// One of the draw's images: the image and color map as sRGB, the others
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

// One of the draw's images read at `taps` and blended, `dx` and `dy` the
// derivatives of the plain coordinates.
fn sample_taps(images: vec3<u32>, which: u32, taps: Taps, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    var sum = vec4(0.0);
    for (var k = 0u; k < taps.count; k++) {
        let turn = taps.turn[k];
        sum += taps.weight[k] * sample_map(images, which, taps.uv[k], turned(dx, turn), turned(dy, turn));
    }
    return sum;
}

// The normal map read at `taps` (X right and Y up the image, Z out), each
// copy's normal turned back by its turn so its slopes face the way the
// copy's features do on the surface, then blended.
fn normal_taps(images: vec3<u32>, taps: Taps, dx: vec2<f32>, dy: vec2<f32>) -> vec3<f32> {
    var sum = vec3(0.0);
    for (var k = 0u; k < taps.count; k++) {
        let turn = taps.turn[k];
        let t = sample_map(images, NORMAL_MAP, taps.uv[k], turned(dx, turn), turned(dy, turn)).xyz * 2.0 - 1.0;
        // The image's Y runs up, its coordinates down.
        let back = vec2(t.x * turn.x + t.y * turn.y, t.y * turn.x - t.x * turn.y);
        sum += taps.weight[k] * vec3(back, t.z);
    }
    return sum;
}

// A terrain layer's height (blue of its roughness-metalness map) at its
// `taps`, from its slot in the table. Without a table only the draw's own
// images are bound, so every layer counts as full height and seams blend
// by their corners' weights alone.
@if(image_table)
fn layer_height(slot: u32, taps: Taps, dx: vec2<f32>, dy: vec2<f32>) -> f32 {
    var sum = 0.0;
    for (var k = 0u; k < taps.count; k++) {
        let turn = taps.turn[k];
        sum += taps.weight[k]
            * textureSampleGrad(images_linear[slot], image_sampler, taps.uv[k], turned(dx, turn), turned(dy, turn)).b;
    }
    return sum;
}
@else
fn layer_height(slot: u32, taps: Taps, dx: vec2<f32>, dy: vec2<f32>) -> f32 {
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

// Where direction `d` falls on skybox image `face` (`Globals::sky_faces`),
// half a texel off its edges, as the images don't repeat.
fn sky_uv(face: u32, d: vec3<f32>) -> vec2<f32> {
    let dir = g.sky_faces[face * 3u].xyz;
    let right = g.sky_faces[face * 3u + 1u].xyz;
    let up = g.sky_faces[face * 3u + 2u].xyz;
    let on_face = d / max(dot(d, dir), 1e-6);
    let uv = vec2(0.5 + 0.5 * dot(on_face, right), 0.5 - 0.5 * dot(on_face, up));
    let half_texel = 0.5 / vec2<f32>(sky_size(face));
    return clamp(uv, half_texel, vec2(1.0) - half_texel);
}

// The sky's linear light towards `d`: the skybox image facing nearest it,
// at full detail as Studio's water samples its sky (its smooth water
// shaders read the sky at mip 0; reflections pick their face per pixel, so
// there are no derivatives to pick a mip by), or the sky's color from the horizon to
// overhead before its images are up and without the image table.
@if(image_table)
fn sky_toward(d: vec3<f32>) -> vec3<f32> {
    var face = 0u;
    var along = -2.0;
    for (var f = 0u; f < 6u; f++) {
        let a = dot(d, g.sky_faces[f * 3u].xyz);
        if a > along {
            along = a;
            face = f;
        }
    }
    if sky_slot(face) == 0u {
        return plain_sky_toward(d);
    }
    return textureSampleLevel(images_srgb[sky_slot(face)], image_sampler, sky_uv(face, d), 0.0).rgb;
}
@else
fn sky_toward(d: vec3<f32>) -> vec3<f32> {
    return plain_sky_toward(d);
}

fn plain_sky_toward(d: vec3<f32>) -> vec3<f32> {
    return srgb_to_linear(mix(g.sky_horizon.rgb, g.sky_top.rgb, clamp(d.y, 0.0, 1.0)));
}
