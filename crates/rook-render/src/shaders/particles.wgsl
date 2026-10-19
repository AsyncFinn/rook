// Particles (renderer/particles.rs): each a square of its image facing
// the camera, its colour premultiplied by its opacity, its coverage in
// alpha. Lit ones take the light a surface facing the camera would.
// Unlit ones are their image times colour, squared, times Brightness, in
// light units, and fog fades them out rather than tinting them (Studio's
// particle shaders, `shaders_glsl3.pack` 0.743).

import package::common::g;
import package::tone::{albedo_units, frame};
import package::images::sample_image;
import package::lighting::{diffuse_light, fog_amount, fog_light, sun_light};

struct ParticleIn {
    @builtin(vertex_index) corner: u32,
    @location(0) position: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
    @location(3) rotation: f32,
    @location(4) lit: f32,
    @location(5) image: u32,
    @location(6) brightness: f32,
    @location(7) keep: f32,
};

struct ParticleOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
    @location(3) @interpolate(flat) image: u32,
    @location(4) @interpolate(flat) lit: f32,
    // Brightness and 1 - LightEmission.
    @location(5) @interpolate(flat) look: vec2<f32>,
};

@vertex
fn vs_particle(v: ParticleIn) -> ParticleOut {
    // Two triangles: corners (0,0) (1,0) (0,1), (0,1) (1,0) (1,1).
    let corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    );
    let c = corners[v.corner];
    let to_camera = normalize(g.camera_pos.xyz - v.position);
    var right = cross(vec3(0.0, 1.0, 0.0), to_camera);
    if dot(right, right) < 1e-6 {
        right = vec3(1.0, 0.0, 0.0);
    }
    right = normalize(right);
    let up = cross(to_camera, right);
    let angle = radians(v.rotation);
    let local = (c - 0.5) * v.size;
    let turned = vec2(
        local.x * cos(angle) - local.y * sin(angle),
        local.x * sin(angle) + local.y * cos(angle),
    );
    let world = v.position + right * turned.x + up * turned.y;
    var out: ParticleOut;
    out.clip = g.view_proj * vec4(world, 1.0);
    out.uv = vec2(c.x, 1.0 - c.y);
    out.color = v.color;
    out.world = world;
    out.image = v.image;
    out.lit = v.lit;
    out.look = vec2(v.brightness, v.keep);
    return out;
}

@fragment
fn fs_particle(in: ParticleOut) -> @location(0) vec4<f32> {
    let texel = sample_image(vec3(in.image, 0u, 0u), in.uv);
    let c = in.color.rgb;
    var units = albedo_units(texel.rgb) * c * c * in.look.x;
    var opacity = in.color.a * texel.a;
    let f = fog_amount(in.world);
    if in.lit > 0.0 {
        // Lit: the light around it, then fog towards its colour as for a
        // surface (Approximation).
        let n = normalize(g.camera_pos.xyz - in.world);
        units = mix(diffuse_light(units, in.world, n, sun_light(in.world, n)), fog_light(), f);
    } else {
        opacity *= 1.0 - f;
    }
    // Premultiplied in the frame's encoding: what covers blends over what's
    // behind as Studio blends, and what has no coverage (LightEmission)
    // adds its light.
    return vec4(frame(units * g.fog_range.z) * opacity, opacity * in.look.y);
}
