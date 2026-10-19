// Particles (renderer/particles.rs): each a square of its image facing
// the camera, its colour premultiplied by its opacity, its coverage in
// alpha. Lit ones take the light a surface facing the camera would.

import package::common::g;
import package::tone::{frame, light_units3, srgb_to_linear};
import package::images::sample_image;
import package::lighting::{diffuse_light, fog_amount, sun_light};

struct ParticleIn {
    @builtin(vertex_index) corner: u32,
    @location(0) position: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
    @location(3) rotation: f32,
    @location(4) lit: f32,
    @location(5) image: u32,
};

struct ParticleOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
    @location(3) @interpolate(flat) image: u32,
    @location(4) @interpolate(flat) lit: f32,
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
    return out;
}

@fragment
fn fs_particle(in: ParticleOut) -> @location(0) vec4<f32> {
    let texel = sample_image(vec3(in.image, 0u, 0u), in.uv);
    // Premultiplied light units: lit particles take the light around them,
    // unlit ones keep the look fitted on Studio's pixels.
    var rgb = in.color.rgb * texel.rgb * texel.a;
    let coverage = in.color.a * texel.a;
    if in.lit > 0.0 {
        let n = normalize(g.camera_pos.xyz - in.world);
        rgb = diffuse_light(rgb, in.world, n, sun_light(in.world, n));
    } else if coverage > 1e-4 {
        rgb = light_units3(rgb / coverage) * coverage;
    } else {
        rgb = light_units3(rgb);
    }
    // Fog takes over as for a surface: towards its colour where the
    // particle covers, and its light fading out.
    let f = fog_amount(in.world);
    rgb = mix(rgb, light_units3(srgb_to_linear(g.fog_color.rgb)) * coverage, f) * g.fog_range.z;
    // Premultiplied in the frame's encoding: what covers blends over what's
    // behind as Studio blends, and what has no coverage (LightEmission)
    // adds its light.
    if coverage > 1e-4 {
        return vec4(frame(rgb / coverage) * coverage, coverage);
    }
    return vec4(frame(rgb), 0.0);
}
