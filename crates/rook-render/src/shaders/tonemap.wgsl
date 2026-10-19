// The last pass: the frame (Studio's encoding of light units, `tone.wgsl`)
// through the place's tone curve, onto the screen.

import package::tone::{FRAME_LIGHT, srgb_to_linear, studio_light3};

@group(0) @binding(0) var frame_texture: texture_2d<f32>;
// x: the curve (0 Default, 1 Retro).
@group(0) @binding(1) var<uniform> tone: vec4<f32>;

@vertex
fn vs_tonemap(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let xy = vec2(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    return vec4(xy, 0.0, 1.0);
}

@fragment
fn fs_tonemap(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let v = textureLoad(frame_texture, vec2<i32>(at.xy), 0).rgb;
    let units = v * v * FRAME_LIGHT;
    // Retro shows the light's square root as the pixel, with no curve
    // (measured: black Decals over white Neon draw 510 x Transparency).
    if tone.x == 1.0 {
        return vec4(srgb_to_linear(min(sqrt(units), vec3(1.0))), 1.0);
    }
    return vec4(studio_light3(units), 1.0);
}
