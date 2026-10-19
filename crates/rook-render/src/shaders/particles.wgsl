// Particles (renderer/particles.rs): each a rectangle of its image
// (facing the camera, or lined up with its velocity), its colour
// premultiplied by its opacity, its coverage in alpha. Lit ones take the light a surface facing the camera would.
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
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) rotation: f32,
    @location(4) lit: f32,
    @location(5) image: u32,
    @location(6) brightness: f32,
    @location(7) keep: f32,
    // The part of the image it shows: offset, scale.
    @location(8) frame: vec4<f32>,
    @location(9) velocity: vec3<f32>,
    // 0 facing the camera, 1 the same turning about world up only, 2
    // lengthwise along its velocity, 3 square on to it.
    @location(10) orientation: u32,
    @location(11) z_offset: f32,
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
    let offset = g.camera_pos.xyz - v.position;
    let distance = length(offset);
    let to_camera = offset / max(distance, 1e-6);
    let world_up = vec3(0.0, 1.0, 0.0);
    var right = cross(world_up, to_camera);
    if dot(right, right) < 1e-6 {
        right = vec3(1.0, 0.0, 0.0);
    }
    right = normalize(right);
    var up = cross(to_camera, right);
    let speed = length(v.velocity);
    if v.orientation == 1u {
        up = world_up;
    } else if v.orientation >= 2u && speed > 1e-4 {
        let along = v.velocity / speed;
        var side = cross(along, to_camera);
        if dot(side, side) < 1e-6 {
            side = cross(along, world_up);
        }
        if dot(side, side) < 1e-6 {
            side = vec3(1.0, 0.0, 0.0);
        }
        side = normalize(side);
        if v.orientation == 2u {
            right = side;
            up = along;
        } else {
            right = side;
            up = cross(along, side);
        }
    }
    // ZOffset moves it towards the camera, the same size on screen.
    let shrink = max(distance - v.z_offset, 0.0) / max(distance, 1e-6);
    let centre = v.position + to_camera * v.z_offset;
    let angle = radians(v.rotation);
    let local = (c - 0.5) * v.size * shrink;
    let turned = vec2(
        local.x * cos(angle) - local.y * sin(angle),
        local.x * sin(angle) + local.y * cos(angle),
    );
    let world = centre + right * turned.x + up * turned.y;
    var out: ParticleOut;
    out.clip = g.view_proj * vec4(world, 1.0);
    out.uv = v.frame.xy + vec2(c.x, 1.0 - c.y) * v.frame.zw;
    out.color = v.color;
    out.world = world;
    out.image = v.image;
    out.lit = v.lit;
    out.look = vec2(v.brightness, v.keep);
    return out;
}

// A Beam's or Trail's vertex (renderer/ribbons.rs), drawn as particles
// are.
struct RibbonIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) lit: f32,
    @location(4) image: u32,
    @location(5) brightness: f32,
    @location(6) keep: f32,
};

@vertex
fn vs_ribbon(v: RibbonIn) -> ParticleOut {
    var out: ParticleOut;
    out.clip = g.view_proj * vec4(v.position, 1.0);
    out.uv = v.uv;
    out.color = v.color;
    out.world = v.position;
    out.image = v.image;
    out.lit = v.lit;
    out.look = vec2(v.brightness, v.keep);
    return out;
}

@fragment
fn fs_particle(in: ParticleOut) -> @location(0) vec4<f32> {
    let texel = sample_image(vec3(in.image, 0u, 0u), in.uv);
    let c = in.color.rgb;
    let albedo = albedo_units(texel.rgb) * c * c;
    var units = albedo * in.look.x;
    var opacity = in.color.a * texel.a;
    let f = fog_amount(in.world);
    // LightInfluence mixes its own colour, which fog fades out, with the
    // light around it, which fog tints as for a surface (Approximation).
    if in.lit > 0.0 {
        let n = normalize(g.camera_pos.xyz - in.world);
        let lit = mix(diffuse_light(albedo, in.world, n, sun_light(in.world, n)), fog_light(), f);
        units = mix(units, lit, in.lit);
    }
    opacity *= mix(1.0 - f, 1.0, in.lit);
    // Premultiplied in the frame's encoding: what covers blends over what's
    // behind as Studio blends, and what has no coverage (LightEmission)
    // adds its light.
    return vec4(frame(units * g.fog_range.z) * opacity, opacity * in.look.y);
}
