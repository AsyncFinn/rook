// Terrain's moving surfaces: water's waves and normal frames, and grass
// blades' sway and lean (docs/behavior/terrain.md). What Studio 0.742
// shows was measured; the shapes chosen to match it are Approximations.

import package::common::{
    g, image_sampler, water_normals, VertexOut, TERRAIN_WATER, TERRAIN_BLADES, srgb_to_linear,
};
import package::lighting::{ambient_light, local_light, sun_light, sun_visibility, fog};

// Studs per repeat of water's normal frames (Approximation: Studio's
// ripples are 1 to 3 studs across, about two of the maps' bumps, and
// don't repeat within 43 studs).
const WATER_NORMAL_STUDS: f32 = 44.0;
// Frames per unit of water's phase (measured: the 25 frames come round
// every 20 units, 2 seconds at WaterWaveSpeed 10).
const WATER_FRAMES_PER_PHASE: f32 = 1.25;

// The waves at `p` (world X and Z): two sines across the water, together
// up to WaterWaveSize studs above and below it, moving with water's phase.
// Returns the height and its slope along X and Z. Observed in Studio
// 0.742: the waves move the surface itself, at WaterWaveSize 1 by about a
// stud, in humps some 50 to 60 studs long.
fn wave(p: vec2<f32>) -> vec3<f32> {
    let size = g.terrain.y;
    // Periods of 40 and 25 units of phase, which divide the period the
    // renderer wraps it at (`WATER_PHASE_PERIOD`).
    let t = g.wind.w * 6.2831853;
    // Wave vectors in radians per stud: wavelengths of 60 and 45 studs.
    let a = vec2(0.095, 0.045);
    let b = vec2(-0.05, 0.13);
    let pa = dot(p, a) + t / 40.0;
    let pb = dot(p, b) + t / 25.0;
    let h = size * 0.5 * (sin(pa) + sin(pb));
    let d = size * 0.5 * (cos(pa) * a + cos(pb) * b);
    return vec3(h, d);
}

// Where a terrain vertex at `world` is drawn: water moved by the waves;
// a grass blade's vertex (`uv`: a number from 0 to 1 for its blade, its
// height above the blade's foot) swayed, leant into GlobalWind, and sunk
// into the ground as blades thin out from 60 studs away (observed:
// Studio's blades are as dense at 100 studs, sparser at 200) and towards
// the edge of where grass is drawn.
fn terrain_motion(world: vec3<f32>, packed: u32, uv: vec2<f32>) -> vec3<f32> {
    var p = world;
    if (packed & TERRAIN_WATER) != 0u {
        p.y += wave(p.xz).x;
    }
    if (packed & TERRAIN_BLADES) != 0u {
        let h = uv.y;
        let wind = g.wind.xz;
        let speed = length(wind);
        let dir = select(vec2(1.0, 0.0), wind / max(speed, 1e-3), speed > 1e-3);
        // A wave of sway across the grass (observed with no wind: tips 3
        // studs up swing about 0.1 studs either way, over about 3
        // seconds, blades a stud apart in step, 1.3 apart opposite).
        let sway = sin(g.terrain.x * 2.1 + dot(p.xz, dir) * 2.4 + uv.x * 1.5) * 0.035 * h;
        // Wind turns each blade about its foot towards where it blows
        // (observed: about 12 degrees at 5 studs per second, 45 at 20, 50
        // at 60).
        let lean = 0.8727 * tanh(speed / 15.0);
        let side = vec3(dir.x, 0.0, dir.y);
        p += side * (sin(lean) * h + sway) - vec3(0.0, (1.0 - cos(lean)) * h, 0.0);
        let reach = g.terrain.w;
        let dist = distance(p, g.camera_pos.xyz);
        var grow = clamp((reach - dist) / (reach * 0.25), 0.0, 1.0);
        if uv.x > 1.0 - 0.6 * smoothstep(60.0, reach, dist) {
            grow = 0.0;
        }
        p.y -= (h + 0.1) * (1.0 - grow);
    }
    return p;
}

// A water normal frame's slope at `uv` (X in alpha, Y in green).
fn water_frame(uv: vec2<f32>, frame: i32, dx: vec2<f32>, dy: vec2<f32>) -> vec2<f32> {
    return textureSampleGrad(water_normals, image_sampler, uv, frame, dx, dy).ag * 2.0 - 1.0;
}

// Water's surface: WaterColor lit by its rippled normal, the sky reflected
// (half of WaterReflectance looking down, all of it at grazing angles, by
// Schlick's Fresnel; the sky's colour from overhead to the horizon by
// where the reflection points, so the ripples show, as Studio's do when it
// reflects its skybox), the sun's glint, and more opaque at grazing
// angles.
// Its normal follows the waves, rippled by the normal frames, which play
// at WATER_FRAMES_PER_PHASE, each fading into the next (Inference: shots a
// third of a frame apart differ as much as the next third does), and stay
// in place (measured: they drift less than 0.07 studs per unit).
fn water_surface(in: VertexOut, dp_x: vec3<f32>, dp_y: vec3<f32>) -> vec4<f32> {
    let w = wave(in.world.xz);
    var n = normalize(vec3(-w.y, 1.0, -w.z));
    if in.normal.y < 0.5 {
        // Water's sides don't wave.
        n = normalize(in.normal);
    }
    let frames = f32(textureNumLayers(water_normals));
    let f = g.wind.w * WATER_FRAMES_PER_PHASE;
    let f0 = floor(f);
    let uv = in.world.xz / WATER_NORMAL_STUDS;
    let dx = dp_x.xz / WATER_NORMAL_STUDS;
    let dy = dp_y.xz / WATER_NORMAL_STUDS;
    let s0 = water_frame(uv, i32(f0 % frames), dx, dy);
    let s1 = water_frame(uv, i32((f0 + 1.0) % frames), dx, dy);
    let s = mix(s0, s1, f - f0);
    let strength = clamp(g.terrain.y * 2.0 + 0.2, 0.2, 1.0) * max(n.y, 0.0);
    n = normalize(n + vec3(s.x, 0.0, s.y) * strength);
    let v = normalize(g.camera_pos.xyz - in.world);
    let fresnel = 0.02 + 0.98 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
    let up = vec3(0.0, 1.0, 0.0);
    let body = in.color.rgb * (ambient_light(n) + local_light(in.world, n) + vec3(sun_light(in.world, n)));
    let reflected = clamp(in.material.x * mix(0.5, 1.0, fresnel), 0.0, 1.0);
    let r = reflect(-v, n);
    let sky = mix(srgb_to_linear(g.sky_horizon.rgb), srgb_to_linear(g.sky_top.rgb), clamp(r.y, 0.0, 1.0));
    var c = mix(body, sky, reflected);
    if g.sun_dir.y > 0.0 {
        let glint = pow(max(dot(r, g.sun_dir.xyz), 0.0), 2000.0);
        c += vec3(glint * g.fog_color.w * 0.3 * sun_visibility(in.world, up));
    }
    return vec4(fog(c, in.world), mix(in.color.a, 1.0, fresnel));
}
