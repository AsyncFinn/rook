// The skybox: a full-screen triangle per image (the instance is the image),
// at the far plane.

import package::common::g;
import package::tone::{frame_of, srgb_to_linear};
import package::images::{sample_sky, sky_uv};

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
    let along = dot(d, g.sky_faces[in.face * 3u].xyz);
    let texel = sample_sky(in.face, sky_uv(in.face, d));
    let a = abs(d);
    if along < max(a.x, max(a.y, a.z)) - 1e-6 {
        discard;
    }
    return vec4(frame_of(texel.rgb * g.fog_range.z), 1.0);
}

// The plain sky, before the skybox's images are up (or without any): its
// colour at the horizon over the whole screen.
@fragment
fn fs_plain_sky(in: SkyOut) -> @location(0) vec4<f32> {
    return vec4(frame_of(srgb_to_linear(g.sky_horizon.rgb) * g.fog_range.z), 1.0);
}
