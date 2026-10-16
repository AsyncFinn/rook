// The light on surfaces, shared by parts and terrain: the sun (shadowed
// through the shadow map), Ambient and OutdoorAmbient, and fog.

import package::common::{g, srgb_to_linear};

@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

const PI: f32 = 3.14159265;

// How much of the sun reaches a point: 3x3 comparison samples (each one
// filtered 2x2 by the sampler), fading out towards the edge of the
// shadowed distance.
fn sun_visibility(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let reach = g.shadow.w;
    if reach == 0.0 {
        return 1.0;
    }
    let p = g.shadow_view_proj * vec4(world + n * g.shadow.y, 1.0);
    let uv = vec2(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) || p.z > 1.0 {
        return 1.0;
    }
    let reference = p.z + g.shadow.z;
    var lit = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let offset = vec2(f32(x), f32(y)) * g.shadow.x;
            lit += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + offset, reference);
        }
    }
    let fade = smoothstep(reach * 0.8, reach, distance(world, g.camera_pos.xyz));
    return mix(lit / 9.0, 1.0, fade);
}

// The sun's light reaching a surface with normal `n` (shadowed), times
// the cosine of its angle.
fn sun_light(world: vec3<f32>, n: vec3<f32>) -> f32 {
    var sun = max(dot(n, g.sun_dir.xyz), 0.0) * g.fog_color.w * 0.5;
    if sun > 0.0 {
        sun *= sun_visibility(world, n);
    }
    return sun;
}

// Ambient and OutdoorAmbient (fading on faces pointing down).
fn ambient_light(n: vec3<f32>) -> vec3<f32> {
    let sky = 0.5 + 0.5 * n.y;
    return srgb_to_linear(g.ambient.rgb) + srgb_to_linear(g.outdoor_ambient.rgb) * sky;
}

// Sun, ambient and fog on a linear albedo, without highlights (decals).
fn shade(albedo: vec3<f32>, world: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let n = normalize(normal);
    return fog(albedo * (ambient_light(n) + vec3(sun_light(world, n))), world);
}

// Physically based shading (Approximation, tuned against Studio 0.742,
// docs/behavior/materials.md): the diffuse light of `shade`, a GGX
// highlight from the sun, and the sky reflected with Schlick's Fresnel
// (weaker at grazing angles the rougher the surface), in the albedo's
// colour on metals. The reflection is scaled by EnvironmentSpecularScale
// (`g.sky.w`), and metals lose their diffuse light only as far as it
// reflects: at 0, a Foil part is lit like Plastic (observed). The sky
// reflected is its average colour (`g.sky`), as Reflectance uses.
fn shade_pbr(
    albedo: vec3<f32>,
    world: vec3<f32>,
    normal: vec3<f32>,
    roughness: f32,
    metalness: f32,
) -> vec3<f32> {
    let n = normalize(normal);
    let v = normalize(g.camera_pos.xyz - world);
    let l = g.sun_dir.xyz;
    let sun = sun_light(world, n);
    let f0 = mix(vec3(0.04), albedo, metalness);
    let n_v = max(dot(n, v), 1e-4);
    let environment = g.sky.w;
    var color = albedo * (1.0 - metalness * environment) * (ambient_light(n) + vec3(sun));
    if sun > 0.0 {
        let h = normalize(l + v);
        let n_h = max(dot(n, h), 0.0);
        let n_l = max(dot(n, l), 1e-4);
        let a = max(roughness * roughness, 0.002);
        let a2 = a * a;
        let d = a2 / (PI * pow(n_h * n_h * (a2 - 1.0) + 1.0, 2.0));
        let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
        let vis = 1.0 / (4.0 * (n_l * (1.0 - k) + k) * (n_v * (1.0 - k) + k));
        let f = f0 + (1.0 - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
        // `sun` already holds the cosine; Lambert's 1/pi is folded into
        // the diffuse term, so the highlight takes it back.
        color += d * vis * f * sun * PI;
    }
    let fresnel = f0 + (max(vec3(1.0 - roughness), f0) - f0) * pow(1.0 - n_v, 5.0);
    color += srgb_to_linear(g.sky.rgb) * fresnel * environment;
    return fog(color, world);
}

fn fog(color: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let dist = distance(world, g.camera_pos.xyz);
    let f = clamp((dist - g.fog_range.x) / max(g.fog_range.y - g.fog_range.x, 1e-3), 0.0, 1.0);
    return mix(color, srgb_to_linear(g.fog_color.rgb), f);
}
