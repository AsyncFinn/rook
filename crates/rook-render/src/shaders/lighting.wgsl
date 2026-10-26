// The light on surfaces, shared by parts and terrain: the sun (shadowed
// through the shadow map), the ambient (Ambient and OutdoorAmbient), the
// sky's diffuse light (EnvironmentDiffuseScale), local lights, and fog. All
// in Studio's light units, added up before the tone curve as Studio adds
// them (`tone.wgsl`).

import package::common::{g, sky_cube, brdf_table, image_sampler};

import package::lights::VOXEL;

@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

// Local lights (PointLight, SpotLight, SurfaceLight), as Studio draws them
// in the lighting archived places get: each light writes into 4-stud
// voxels (`light_volume.wgsl` fills them ahead of the camera), and a surface
// takes the voxels around the point 6 studs out along its normal,
// blended, whichever way it faces (docs/behavior/lighting/local-lights.md).
struct LightVolume {
    // xyz: the center of voxel (0, 0, 0); w: 0 when there's no volume.
    origin: vec4<f32>,
    // xyz: voxels along each axis; w: 1 when their alpha holds how much
    // of the sky's diffuse light reaches them.
    size: vec4<f32>,
    // x: 1 when `sun_voxels` hold how much of the sun reaches them.
    sun: vec4<f32>,
};

@group(3) @binding(0) var<uniform> light_volume: LightVolume;
@group(3) @binding(1) var light_voxels: texture_3d<f32>;
@group(3) @binding(2) var light_sampler: sampler;
@group(3) @binding(3) var sun_voxels: texture_3d<f32>;

// How far out along the normal a surface reads the voxels (Studio's
// shaders).
const VOXEL_OFFSET: f32 = 6.0;

// The local lights' light on a surface at `world` with normal `n`, in
// light units: the voxels around the point 6 studs out, blended
// trilinearly.
fn local_light(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    if light_volume.origin.w == 0.0 {
        return vec3(0.0);
    }
    let q = world + normalize(n) * VOXEL_OFFSET;
    let uvw = ((q - light_volume.origin.xyz) / VOXEL + 0.5) / light_volume.size.xyz;
    if any(uvw < vec3(0.0)) || any(uvw > vec3(1.0)) {
        return vec3(0.0);
    }
    return textureSampleLevel(light_voxels, light_sampler, uvw, 0.0).rgb;
}

// How far across the surface `sky_visibility` spreads its samples, in
// studs (fitted).
const SKY_SPREAD: f32 = 3.0;

// The voxels' share of the sky's light at `p`, blended trilinearly; all of
// it outside the volume.
fn sky_voxel(p: vec3<f32>) -> f32 {
    let uvw = ((p - light_volume.origin.xyz) / VOXEL + 0.5) / light_volume.size.xyz;
    if any(uvw < vec3(0.0)) || any(uvw > vec3(1.0)) {
        return 1.0;
    }
    return textureSampleLevel(light_voxels, light_sampler, uvw, 0.0).a;
}

// How much of the sky's diffuse light reaches a surface at `world` facing
// `n` (unit), past the roofs and walls that block it: the voxels around
// the point 6 studs out along the normal, as for local lights, averaged
// over four points 3 studs apart across the surface (Studio's shadows of
// the sky are softer than one voxel blend; fitted, docs/behavior/
// materials.md, "The sky's diffuse light").
fn sky_visibility(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if light_volume.size.w == 0.0 {
        return 1.0;
    }
    let q = world + n * VOXEL_OFFSET;
    let t = normalize(cross(n, select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(n.y) > 0.9)));
    let b = cross(n, t);
    let s = SKY_SPREAD;
    return 0.25 * (sky_voxel(q + (t + b) * s) + sky_voxel(q + (t - b) * s)
        + sky_voxel(q - (t + b) * s) + sky_voxel(q - (t - b) * s));
}

// How far across the surface `voxel_sun_visibility` spreads its samples,
// in studs (fitted).
const SUN_SPREAD: f32 = 4.0;

// The sun voxels' share of the sun at `p`, blended trilinearly; all of it
// outside the volume.
fn sun_voxel(p: vec3<f32>) -> f32 {
    let uvw = ((p - light_volume.origin.xyz) / VOXEL + 0.5) / light_volume.size.xyz;
    if any(uvw < vec3(0.0)) || any(uvw > vec3(1.0)) {
        return 1.0;
    }
    return textureSampleLevel(sun_voxels, light_sampler, uvw, 0.0).r;
}

// How much of the sun reaches a surface at `world` facing `n` (unit)
// without PrioritizeLightingQuality, where Studio shadows the sun through
// its light voxels: soft, blocky, and only within the voxels' reach
// (docs/behavior/lighting/sun.md). The voxels around the point 6 studs out
// along the normal, as for the sky, averaged over a 3x3 pattern
// `SUN_SPREAD` apart across the surface.
fn voxel_sun_visibility(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let q = world + n * VOXEL_OFFSET;
    let t = normalize(cross(n, select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(n.y) > 0.9)));
    let b = cross(n, t);
    var lit = 0.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            lit += sun_voxel(q + (t * f32(i) + b * f32(j)) * SUN_SPREAD);
        }
    }
    return lit / 9.0;
}

// How much of the sun reaches a point: through the shadow map, 3x3
// comparison samples (each one filtered 2x2 by the sampler), fading out
// towards the edge of the shadowed distance; or through the voxels.
fn sun_visibility(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let reach = g.shadow.w;
    if reach == 0.0 {
        if light_volume.sun.x != 0.0 {
            return voxel_sun_visibility(world, n);
        }
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

// The sun's light on a surface with normal `n` (shadowed), in light units:
// `g.sun_color` (0.5 units per Brightness on a white face square to the
// sun, measured) times the cosine of its angle, as Studio lights
// (docs/behavior/lighting/sun.md).
fn sun_light(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var cosine = max(dot(n, g.sun_dir.xyz), 0.0);
    if cosine > 0.0 {
        cosine *= sun_visibility(world, n);
    }
    return g.sun_color.rgb * cosine;
}

// Studio's ambient color: the brighter of Ambient and OutdoorAmbient in
// each channel, the same on every face, indoors or out, whatever the
// Brightness or time of day (docs/behavior/lighting/ambient.md).
fn ambient_color() -> vec3<f32> {
    return max(g.ambient.rgb, g.outdoor_ambient.rgb);
}

// The sky's light left on a surface no sky reaches, in light units at
// EnvironmentDiffuseScale 1 (measured: about 0.04 in each channel under a
// roof, under the default sky and a flat gray one, with Ambient or
// without; it scales with the property).
const SKY_COVERED: f32 = 0.04;

// The sky's diffuse light on a surface facing `n`, in light units (color
// squared), as Studio lights with EnvironmentDiffuseScale (`g.sky_top.w`,
// dimmed at night with the sky): the skybox's average colors overhead,
// around the horizon and below, weighted by how far the surface faces up
// or down. A side face takes 3/4 of the horizon and 1/8 each of the top
// and bottom, an upward face 0.58 of the top and 0.42 of the horizon
// (measured in Studio 0.742 with a red top, blue sides and black bottom;
// docs/behavior/rendering/materials.md, "The sky's diffuse light"), before roofs
// and walls block it (`sky_visibility`).
fn sky_light(n: vec3<f32>) -> vec3<f32> {
    let y = n.y;
    let top = 0.125 + 0.29 * y + 0.165 * y * y;
    let bottom = 0.125 - 0.29 * y + 0.165 * y * y;
    let t = g.sky_top.rgb;
    let h = g.sky_horizon.rgb;
    let b = g.sky_bottom.rgb;
    let light = top * t * t + (1.0 - top - bottom) * h * h + bottom * b * b;
    return light * g.sky_top.w;
}

// The diffuse light off a surface of color `albedo` at `world` facing
// `n`, given the sun's (`sun_light`), in light units: the albedo times the
// ambient (its color squared; measured: white SmoothPlastic under Ambient
// 80 drew 66 under the default curve and 79 under Retro, both 0.093
// units), the sky's light, the local lights' and the sun's. Where roofs
// block the sky, a dim gray light stays (`SKY_COVERED`).
fn diffuse_light(albedo: vec3<f32>, world: vec3<f32>, n: vec3<f32>, sun: vec3<f32>) -> vec3<f32> {
    let a = ambient_color();
    let covered = vec3(SKY_COVERED * g.sky_top.w);
    let sky = mix(covered, sky_light(n), sky_visibility(world, normalize(n)));
    return albedo * (a * a + sky + local_light(world, n) + sun);
}

// Sun, ambient and fog on an albedo, without highlights (decals), in
// light units.
fn shade(albedo: vec3<f32>, world: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let n = normalize(normal);
    return fog_units(diffuse_light(albedo, world, n, sun_light(world, n)), world);
}

// The sun's highlight on a surface of reflectance `f0` at the given
// cosines, per unit of the sun's light on it (its cosine already in):
// Studio's surface shaders' own distribution (`shaders_glsl3.pack`, 0.743:
// remapped roughness r = 0.089 + 0.911 x roughness, a = r^4, no 1/pi and
// no visibility term), with Schlick's Fresnel on the light and half-way
// vectors. Confirmed against black SmoothPlastic at known angles, which
// fits roughness 0.4 within 0.005 units (docs/behavior/lighting/sun.md).
fn sun_highlight(f0: vec3<f32>, roughness: f32, n_h: f32, l_h: f32) -> vec3<f32> {
    let r = remapped(roughness);
    let a = r * r * r * r;
    let d = n_h * n_h * (a - 1.0) + 1.0;
    let f = f0 + (1.0 - f0) * pow(1.0 - l_h, 5.0);
    return f * (a + a * a) / (d * d * (3.0 * l_h + 0.5) * (0.75 * n_h + 0.25));
}

// Studio's remapped roughness, 0.089 to 1: what its highlight, its
// reflection cube's level and its BRDF table go by.
fn remapped(roughness: f32) -> f32 {
    return 0.089 + 0.911 * roughness;
}

// The sky towards `d` in light units, from the reflection cube's `level`
// (0 sharp to 5, Studio's 5 x remapped roughness).
fn sky_reflected(d: vec3<f32>, level: f32) -> vec3<f32> {
    return textureSampleLevel(sky_cube, image_sampler, d, level).rgb;
}

// How much of the sky a surface of reflectance `f0` reflects at remapped
// roughness `r` and cosine `n_v` to the camera: Studio's split-sum table
// (A, B), taken as (f0 A + B) / (A + B), so a rough surface keeps all of
// f0 (`shaders_glsl3.pack`, 0.743).
fn sky_fresnel(f0: vec3<f32>, r: f32, n_v: f32) -> vec3<f32> {
    let half_texel = 0.5 / vec2<f32>(textureDimensions(brdf_table));
    let at = clamp(vec2(r, n_v), half_texel, vec2(1.0) - half_texel);
    let t = textureSampleLevel(brdf_table, image_sampler, at, 0.0).xy;
    return (f0 * t.x + t.y) / max(t.x + t.y, 1e-4);
}

// Physically based shading in light units, as Studio's surface shaders add
// it up (`shaders_glsl3.pack`, 0.743; docs/behavior/lighting/sun.md):
// - the sky reflected towards the camera's mirror direction, from the
//   reflection cube at Studio's level for the roughness (`sky_reflected`),
//   times `sky_fresnel` and EnvironmentSpecularScale (`g.sky_horizon.w`);
//   where the sky is blocked, the dim covered light stands for Studio's
//   indoor cube (Approximation);
// - the sun's highlight (`sun_highlight`);
// - the diffuse light on the albedo: ambient and local lights whole, the
//   sky's light less what its reflection takes, the sun's less what its
//   highlight takes, both only as far as the surface isn't metal.
//   EnvironmentSpecularScale scales metalness too: at 0, a Foil part is
//   lit like Plastic (observed).
// `reflectance` (BasePart.Reflectance, on the materials that honor it)
// mixes the albedo towards the sharp sky in the mirror direction, which is
// then lit like any albedo, and makes the reflection itself sharp.
fn shade_pbr(
    albedo: vec3<f32>,
    world: vec3<f32>,
    normal: vec3<f32>,
    roughness: f32,
    metalness: f32,
    reflectance: f32,
) -> vec3<f32> {
    let n = normalize(normal);
    let v = normalize(g.camera_pos.xyz - world);
    let l = g.sun_dir.xyz;
    let environment = g.sky_horizon.w;
    let metal = metalness * environment;
    let not_metal = 1.0 - metal;
    let f0 = mix(vec3(0.04), albedo, metal);
    let r = remapped(roughness);
    let n_v = max(dot(n, v), 1e-4);
    let level = select(5.0 * r, 0.0, reflectance > 0.0);
    let sky = sky_reflected(reflect(-v, n), level);
    let sky_f = sky_fresnel(f0, r, n_v);
    let sun = sun_light(world, n);
    var sun_kept = vec3(0.0);
    var highlight = vec3(0.0);
    if any(sun > vec3(0.0)) {
        let h = normalize(l + v);
        let l_h = max(dot(l, h), 0.0);
        let f = f0 + (1.0 - f0) * pow(1.0 - l_h, 5.0);
        sun_kept = not_metal * (1.0 - f * environment);
        highlight = sun_highlight(f0, roughness, max(dot(n, h), 1e-3), l_h) * sun;
    }
    let open = sky_visibility(world, n);
    let covered = vec3(SKY_COVERED * g.sky_top.w);
    let sky_diffuse = mix(covered, sky_light(n), open) * not_metal * (1.0 - sky_f * environment);
    let a = ambient_color();
    let surface = mix(albedo, sky, reflectance);
    let diffuse = surface * (a * a + sky_diffuse + local_light(world, n) + sun * sun_kept);
    let reflected = mix(covered, sky, open) * sky_f * environment;
    return fog_units(diffuse + highlight + reflected, world);
}

// How far fog has taken over at `world`, 0 to 1: Studio's surface shaders
// keep exp2(1 - t) - 1 of the surface at t = (distance - FogStart) /
// (FogEnd - FogStart) (their form exp2(z x d + x) - w; the constants fit
// white parts at 12 distances under three FogStart/FogEnd pairs within
// 0.03; docs/behavior/lighting/tone-mapping.md, "Fog").
fn fog_amount(world: vec3<f32>) -> f32 {
    let dist = distance(world, g.camera_pos.xyz);
    let t = (dist - g.fog_range.x) / max(g.fog_range.y - g.fog_range.x, 1e-3);
    return 1.0 - clamp(exp2(1.0 - t) - 1.0, 0.0, 1.0);
}

// FogColor in light units (its color squared, as Studio's fog draws).
fn fog_light() -> vec3<f32> {
    return g.fog_color.rgb * g.fog_color.rgb;
}

// Fog, then the exposure, on light units (every lit surface ends here),
// as Studio's shaders apply them before the frame.
fn fog_units(units: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    return mix(units, fog_light(), fog_amount(world)) * g.fog_range.z;
}
