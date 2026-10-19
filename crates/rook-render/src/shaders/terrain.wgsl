// Terrain's moving surfaces: water's waves and normal frames, and grass
// blades' sway and lean (docs/behavior/terrain.md). What Studio 0.742
// shows was measured; the shapes chosen to match it are Approximations.

import package::common::{
    g, image_sampler, water_normals, VertexOut, TERRAIN_WATER, TERRAIN_BLADES,
    TERRAIN_DRAWN_SHIFT, terrain_taps,
};
import package::images::layer_height;
import package::lighting::{diffuse_light, sun_light, sun_visibility, fog_units};
import package::tone::{albedo_units, frame};

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
    let body = diffuse_light(albedo_units(in.color.rgb), in.world, n, sun_light(in.world, n));
    let reflected = clamp(in.material.x * mix(0.5, 1.0, fresnel), 0.0, 1.0);
    let r = reflect(-v, n);
    let sky = mix(g.sky_horizon.rgb, g.sky_top.rgb, clamp(r.y, 0.0, 1.0));
    var c = mix(body, sky * sky, reflected);
    if g.sun_dir.y > 0.0 {
        // The sun's glint, in light units: as bright as the sun's light on
        // a face square to it (Approximation, unmeasured; it keeps the
        // look fitted before light units at Brightness 2).
        let glint = pow(max(dot(r, g.sun_dir.xyz), 0.0), 2000.0);
        c += glint * g.sun_color.rgb * sun_visibility(in.world, up);
    }
    return vec4(frame(fog_units(c, in.world)), mix(in.color.a, 1.0, fresnel));
}

// A terrain seam fragment's share of its draw's layer (Studio 0.742's
// terrain shader, read from its GLSL): each of the triangle's corners
// weighs its barycentric weight times its layer's height there (remapped
// to 0 to 1); each weight beyond the highest of the others, plus half the
// edge's softness (the corners' softness, blended), over that softness,
// gives the corner's share, the shares then summing to 1. Where the
// textures shrink to under 7 pixels a repeat, the shares fade to the plain
// barycentric weights, as they do near the edge between two corners of
// one layer. The draw's share is that of the corners its layer holds.
// Every draw of a triangle works the shares out alike (the triangle's own
// normal, not its vertices', picks the planes), so they add up to the
// whole.
fn terrain_share(in: VertexOut, dp_x: vec3<f32>, dp_y: vec3<f32>) -> f32 {
    let drawn = in.face >> TERRAIN_DRAWN_SHIFT;
    let b = vec3(in.uv.x, in.uv.y, 1.0 - in.uv.x - in.uv.y);
    let ids = vec3<u32>(round(in.local));
    var n = cross(dp_x, dp_y);
    n = normalize(select(n, -n, dot(n, in.normal) < 0.0));
    var h = vec3(0.0);
    var softness = 0.0;
    var minified = 0.0;
    for (var k = 0u; k < 3u; k++) {
        let a = g.terrain_layers[ids[k] * 2u];
        let c = g.terrain_layers[ids[k] * 2u + 1u];
        let cube = c.y > 0.5;
        let studs = max(a.w, 1e-3);
        let taps = terrain_taps(terrain_position(in.world, n, cube), studs, c.z, c.w);
        // The planes' coordinates are linear in the position.
        let dx = terrain_position(dp_x, n, cube) / studs;
        let dy = terrain_position(dp_y, n, cube) / studs;
        h[k] = clamp(layer_height(u32(c.x), taps, dx, dy) * a.x + a.y, 0.0, 1.0);
        softness += b[k] * a.z;
        if k == 0u {
            minified = sqrt(max(dot(dx, dx), dot(dy, dy))) * 7.0;
        }
    }
    let w = b * h;
    let others = vec3(max(w.y, w.z), max(w.x, w.z), max(w.x, w.y));
    let soft = max(softness, 1e-4);
    var s = clamp((w - others + 0.5 * soft) / soft, vec3(0.0), vec3(1.0));
    s /= max(s.x + s.y + s.z, 1e-4);
    let paired = vec3(
        f32(ids.x == ids.y || ids.x == ids.z),
        f32(ids.y == ids.x || ids.y == ids.z),
        f32(ids.z == ids.x || ids.z == ids.y),
    );
    s = mix(s, b, clamp(minified + dot(paired, b) * 20.0 - 19.0, 0.0, 1.0));
    let mine = vec3(f32(drawn & 1u), f32((drawn >> 1u) & 1u), f32((drawn >> 2u) & 1u));
    return dot(s, mine);
}

// Where a terrain point is on its material's textures, in world studs
// across the plane its surface faces most: of the 6 axes' planes for the
// cube-mapped materials (Brick, WoodPlanks), of 18 for the rest (the 6 and
// the 12 at 45 degrees between two axes), as Studio picks one of 18 planes
// (zeux, SIGGRAPH 2020). The images run right and down across each plane;
// which way exactly isn't known (Approximation).
fn terrain_position(p: vec3<f32>, n: vec3<f32>, cube: bool) -> vec2<f32> {
    let a = abs(n);
    let s = select(vec3(-1.0), vec3(1.0), n >= vec3(0.0));
    var best = a.x;
    var plane = 0u;
    if a.y > best {
        best = a.y;
        plane = 1u;
    }
    if a.z > best {
        best = a.z;
        plane = 2u;
    }
    if !cube {
        let d = vec3(a.x + a.y, a.x + a.z, a.y + a.z) * 0.70710678;
        if d.x > best {
            best = d.x;
            plane = 3u;
        }
        if d.y > best {
            best = d.y;
            plane = 4u;
        }
        if d.z > best {
            plane = 5u;
        }
    }
    let h = 0.70710678;
    switch plane {
        case 0u: { return vec2(-s.x * p.z, -p.y); }
        case 1u: { return vec2(p.x, s.y * p.z); }
        case 2u: { return vec2(s.z * p.x, -p.y); }
        case 3u: { return vec2(p.z, -(s.y * p.y - s.x * p.x) * h); }
        case 4u: { return vec2((s.z * p.x - s.x * p.z) * h, -p.y); }
        default: { return vec2(p.x, -(s.z * p.z - s.y * p.y) * h); }
    }
}
