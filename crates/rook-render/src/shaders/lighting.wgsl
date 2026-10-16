// The light on surfaces, shared by parts and terrain: the sun (shadowed
// through the shadow map), the ambient (Ambient and OutdoorAmbient), local
// lights, and fog.

import package::common::{g, srgb_to_linear};
import package::lights::VOXEL;

@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

const PI: f32 = 3.14159265;

// Local lights (PointLight, SpotLight, SurfaceLight), as Studio draws them
// in the lighting archived places get: each light writes into 4-stud
// voxels (`light_volume.wgsl` fills them ahead of the camera), and a surface
// takes the voxels around the point 6 studs out along its normal,
// blended, whichever way it faces (docs/behavior/lights.md).
struct LightVolume {
    // xyz: the centre of voxel (0, 0, 0); w: 0 when there's no volume.
    origin: vec4<f32>,
    // xyz: voxels along each axis.
    size: vec4<f32>,
};

@group(3) @binding(0) var<uniform> light_volume: LightVolume;
@group(3) @binding(1) var light_voxels: texture_3d<f32>;
@group(3) @binding(2) var light_sampler: sampler;

// How far out along the normal a surface reads the voxels (Studio's
// shaders).
const VOXEL_OFFSET: f32 = 6.0;

// Studio's tone curve as Rook draws it: its light units (Ambient's colour
// squared, a light's Brightness x Color squared) to the linear light that
// draws a white surface as Studio does (measured on white SmoothPlastic:
// Ambient 32, 64, 96, 128, 160, 192 and 255 grey drew 14, 47, 91, 134,
// 166, 193 and 224; brighter lights up to 253).
fn studio_light(v: f32) -> f32 {
    let x = array<f32, 11>(0.0, 0.0156, 0.0625, 0.141, 0.25, 0.393, 0.567, 1.0, 2.24, 3.5, 8.0);
    let y = array<f32, 11>(0.0, 0.00437, 0.0284, 0.104, 0.239, 0.381, 0.533, 0.745, 0.939, 0.982, 1.0);
    if v >= x[10] {
        return y[10];
    }
    var i = 0;
    while i < 9 && v > x[i + 1] {
        i++;
    }
    return mix(y[i], y[i + 1], clamp((v - x[i]) / (x[i + 1] - x[i]), 0.0, 1.0));
}

fn studio_light3(v: vec3<f32>) -> vec3<f32> {
    return vec3(studio_light(v.x), studio_light(v.y), studio_light(v.z));
}

// The local lights' light on a surface at `world` with normal `n`, in
// Rook's linear light: the voxels around the point 6 studs out, blended
// trilinearly, through Studio's tone curve on top of the Ambient it adds
// to (so a lit wall in a lit room brightens as Studio's does).
fn local_light(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    if light_volume.origin.w == 0.0 {
        return vec3(0.0);
    }
    let q = world + normalize(n) * VOXEL_OFFSET;
    let uvw = ((q - light_volume.origin.xyz) / VOXEL + 0.5) / light_volume.size.xyz;
    if any(uvw < vec3(0.0)) || any(uvw > vec3(1.0)) {
        return vec3(0.0);
    }
    let sum = textureSampleLevel(light_voxels, light_sampler, uvw, 0.0).rgb;
    // Exposure scales light before the curve, as in Studio; `fog` applies
    // it to the rest afterwards, so it's taken back out here.
    let e = g.fog_range.z;
    let a = ambient_color();
    let ambient = a * a * e;
    return max(studio_light3(ambient + sum * e) - studio_light3(ambient), vec3(0.0)) / e;
}

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

// Studio's ambient colour: the brighter of Ambient and OutdoorAmbient in
// each channel, the same on every face, indoors or out, whatever the
// Brightness or time of day (docs/behavior/lights.md, "Ambient and
// OutdoorAmbient").
fn ambient_color() -> vec3<f32> {
    return max(g.ambient.rgb, g.outdoor_ambient.rgb);
}

// The ambient light on every surface, in Rook's linear light.
fn ambient_light() -> vec3<f32> {
    return srgb_to_linear(ambient_color());
}

// Sun, ambient and fog on a linear albedo, without highlights (decals).
fn shade(albedo: vec3<f32>, world: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let n = normalize(normal);
    return fog(albedo * (ambient_light() + local_light(world, n) + vec3(sun_light(world, n))), world);
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
    let diffuse = ambient_light() + local_light(world, n) + vec3(sun);
    var color = albedo * (1.0 - metalness * environment) * diffuse;
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

// Fog, then the exposure (every lit surface ends here).
fn fog(color: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let dist = distance(world, g.camera_pos.xyz);
    let f = clamp((dist - g.fog_range.x) / max(g.fog_range.y - g.fog_range.x, 1e-3), 0.0, 1.0);
    return mix(color, srgb_to_linear(g.fog_color.rgb), f) * g.fog_range.z;
}
