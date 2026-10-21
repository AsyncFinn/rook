// The sky's reflection cube (`renderer/lighting/sky.rs`), built as Studio
// builds its prefiltered environment map (`shaders_glsl3.pack`, 0.743):
// the sky drawn onto a cube in light units, box mips of it, and the
// reflection cube's level m (1 to 5) the sky seen through Studio's
// highlight distribution at roughness m / 5, from 32 directions read from
// the sky's level m - 1 and weighted by their cosine. Surfaces read level
// 5 x their remapped roughness (`sky_reflected` in `lighting.wgsl`).

struct Params {
    // Each skybox image's direction, right and up (`SKYBOX_FACES`).
    faces: array<vec4<f32>, 18>,
    // The plain sky overhead and at the horizon (sRGB), drawn without
    // images. flags: x, the six images are bound; y, the cube's size.
    top: vec4<f32>,
    horizon: vec4<f32>,
    flags: vec4<u32>,
};

@group(0) @binding(0) var cube_sampler: sampler;
@group(0) @binding(1) var<uniform> params: Params;
// The skybox's images in `SKYBOX_FACES` order, as stored (sRGB bytes).
@group(0) @binding(2) var face0: texture_2d<f32>;
@group(0) @binding(3) var face1: texture_2d<f32>;
@group(0) @binding(4) var face2: texture_2d<f32>;
@group(0) @binding(5) var face3: texture_2d<f32>;
@group(0) @binding(6) var face4: texture_2d<f32>;
@group(0) @binding(7) var face5: texture_2d<f32>;
// The level a pass reads: the one before for box mips, the whole chain
// for the reflection cube.
@group(0) @binding(8) var source: texture_cube<f32>;

struct CubeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // The cube face drawn (wgpu's order: +X, -X, +Y, -Y, +Z, -Z) and,
    // above it, the level (instance = level x 6 + face).
    @location(1) @interpolate(flat) at: u32,
};

@vertex
fn vs_cube(@builtin(vertex_index) i: u32, @builtin(instance_index) at: u32) -> CubeOut {
    let xy = vec2(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var out: CubeOut;
    out.clip = vec4(xy, 0.0, 1.0);
    out.uv = vec2(xy.x, -xy.y) * 0.5 + 0.5;
    out.at = at;
    return out;
}

// The direction through `uv` on cube face `face`, as wgpu samples cubes.
fn cube_direction(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let s = uv.x * 2.0 - 1.0;
    let t = uv.y * 2.0 - 1.0;
    switch face {
        case 0u: { return normalize(vec3(1.0, -t, -s)); }
        case 1u: { return normalize(vec3(-1.0, -t, s)); }
        case 2u: { return normalize(vec3(s, 1.0, t)); }
        case 3u: { return normalize(vec3(s, -1.0, -t)); }
        case 4u: { return normalize(vec3(s, -t, 1.0)); }
        default: { return normalize(vec3(-s, -t, -1.0)); }
    }
}

fn sample_face(face: u32, uv: vec2<f32>, level: f32) -> vec3<f32> {
    switch face {
        case 0u: { return textureSampleLevel(face0, cube_sampler, uv, level).rgb; }
        case 1u: { return textureSampleLevel(face1, cube_sampler, uv, level).rgb; }
        case 2u: { return textureSampleLevel(face2, cube_sampler, uv, level).rgb; }
        case 3u: { return textureSampleLevel(face3, cube_sampler, uv, level).rgb; }
        case 4u: { return textureSampleLevel(face4, cube_sampler, uv, level).rgb; }
        default: { return textureSampleLevel(face5, cube_sampler, uv, level).rgb; }
    }
}

fn face_size(face: u32) -> f32 {
    switch face {
        case 0u: { return f32(textureDimensions(face0).x); }
        case 1u: { return f32(textureDimensions(face1).x); }
        case 2u: { return f32(textureDimensions(face2).x); }
        case 3u: { return f32(textureDimensions(face3).x); }
        case 4u: { return f32(textureDimensions(face4).x); }
        default: { return f32(textureDimensions(face5).x); }
    }
}

// The sky towards `d` in light units: the skybox image facing nearest it
// (as `fs_sky` draws it), its sRGB color squared as Studio's shaders take
// colors, or the plain sky from the horizon to overhead.
@fragment
fn fs_capture(in: CubeOut) -> @location(0) vec4<f32> {
    let d = cube_direction(in.at % 6u, in.uv);
    if params.flags.x == 0u {
        let c = mix(params.horizon.rgb, params.top.rgb, clamp(d.y, 0.0, 1.0));
        return vec4(c * c, 1.0);
    }
    var face = 0u;
    var along = -2.0;
    for (var f = 0u; f < 6u; f++) {
        let a = dot(d, params.faces[f * 3u].xyz);
        if a > along {
            along = a;
            face = f;
        }
    }
    let on_face = d / along;
    let right = params.faces[face * 3u + 1u].xyz;
    let up = params.faces[face * 3u + 2u].xyz;
    let size = face_size(face);
    let half_texel = 0.5 / size;
    let uv = clamp(
        vec2(0.5 + 0.5 * dot(on_face, right), 0.5 - 0.5 * dot(on_face, up)),
        vec2(half_texel),
        vec2(1.0 - half_texel),
    );
    // The image's level nearest the cube's texels.
    let level = max(log2(size / f32(params.flags.y)), 0.0);
    let c = sample_face(face, uv, level);
    return vec4(c * c, 1.0);
}

// A box mip: the level before, read between its four texels.
@fragment
fn fs_downsample(in: CubeOut) -> @location(0) vec4<f32> {
    let d = cube_direction(in.at % 6u, in.uv);
    return textureSampleLevel(source, cube_sampler, d, 0.0);
}

// Directions read per texel of the reflection cube.
const SAMPLES: u32 = 32u;

// `i`'s bits reversed, over SAMPLES: the second coordinate of Hammersley's
// points.
fn radical_inverse(i: u32) -> f32 {
    return f32(reverseBits(i) >> 27u) / f32(SAMPLES);
}

// The reflection cube's level `at / 6` (1 to 5): the sky through the
// highlight distribution at roughness level / 5 (a^4 = roughness^4), its
// half-way vectors at Hammersley's points (turned by 2 pi i / 32, cosines
// sqrt((1 - x) / (1 + (a^4 - 1) x))), each reflected direction weighted
// by its cosine.
@fragment
fn fs_prefilter(in: CubeOut) -> @location(0) vec4<f32> {
    let n = cube_direction(in.at % 6u, in.uv);
    let level = f32(in.at / 6u);
    let r = level / 5.0;
    let a4 = r * r * r * r;
    let up = select(vec3(1.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0), abs(n.z) < 0.999);
    let tangent = normalize(cross(up, n));
    let bitangent = cross(n, tangent);
    var sum = vec3(0.0);
    var weight = 0.0;
    for (var i = 0u; i < SAMPLES; i++) {
        let x = radical_inverse(i);
        let phi = 6.2831853 * f32(i) / f32(SAMPLES);
        let cos_theta = sqrt((1.0 - x) / (1.0 + (a4 - 1.0) * x));
        let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
        let h = tangent * (sin_theta * cos(phi)) + bitangent * (sin_theta * sin(phi)) + n * cos_theta;
        let l = h * (2.0 * dot(n, h)) - n;
        let w = clamp(dot(n, l), 0.0, 1.0);
        sum += textureSampleLevel(source, cube_sampler, l, level - 1.0).rgb * w;
        weight += w;
    }
    return vec4(sum / weight, 1.0);
}
