// The skybox: a full-screen triangle per image (the instance is the image),
// at the far plane.

import package::common::g;
import package::images::{sample_sky, sky_size};

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
    @location(1) @interpolate(flat) face: u32,
};

@vertex
fn vs_sky(@builtin(vertex_index) i: u32, @builtin(instance_index) face: u32) -> SkyOut {
    let xy = vec2(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var out: SkyOut;
    out.clip = vec4(xy, 0.0, 1.0);
    out.ndc = xy;
    out.face = face;
    return out;
}

// One skybox image: the pixels whose direction is closest to its face's,
// mapped straight onto the cube face (unlit and unfogged).
@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let p = g.sky_from_clip * vec4(in.ndc, 1.0, 1.0);
    let d = normalize(p.xyz / p.w);
    let dir = g.sky_faces[in.face * 3u].xyz;
    let right = g.sky_faces[in.face * 3u + 1u].xyz;
    let up = g.sky_faces[in.face * 3u + 2u].xyz;
    let along = dot(d, dir);
    let on_face = d / max(along, 1e-6);
    var uv = vec2(0.5 + 0.5 * dot(on_face, right), 0.5 - 0.5 * dot(on_face, up));
    // Images don't repeat: keep the sampler off the far edges.
    let half_texel = 0.5 / vec2<f32>(sky_size(in.face));
    uv = clamp(uv, half_texel, vec2(1.0) - half_texel);
    let texel = sample_sky(in.face, uv);
    let a = abs(d);
    if along < max(a.x, max(a.y, a.z)) - 1e-6 {
        discard;
    }
    return vec4(texel.rgb * g.fog_range.z, 1.0);
}
