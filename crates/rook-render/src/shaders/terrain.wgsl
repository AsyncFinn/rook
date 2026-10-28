// Terrain's own looks: its textures recolored by MaterialColors
// (docs/behavior/terrain/materials.md#colors), water's waves and normal
// frames, and grass blades' light, sway and lean
// (docs/behavior/terrain/grass.md). What Studio 0.742 shows was measured;
// the shapes chosen to match it are Approximations.

import package::common::{
    g, image_sampler, water_normals, VertexOut, TERRAIN_WATER, TERRAIN_BLADES,
    TERRAIN_DRAWN_SHIFT, terrain_taps,
};
import package::images::{layer_height, sky_toward};
import package::lighting::{diffuse_light, sun_light, sun_visibility, fog_units};
import package::tone::{albedo_units, frame, light_units3};

// A texel of a terrain color map (sRGB) recolored by its material's color,
// as Studio builds its YCoCg texture array (`rook_terrain::Recolor`; `r`:
// the gain, then the turn): the luma times the gain, at most 1, and the
// chroma turned and scaled, within 0.5 either way. Channels may pass 1,
// as Studio's do.
fn recolor(s: vec3<f32>, r: vec3<f32>) -> vec3<f32> {
    let y = clamp(dot(s, vec3(0.25, 0.5, 0.25)) * r.x, 0.0, 1.0);
    let co = (s.r - s.b) * 0.5;
    let cg = dot(s, vec3(-0.25, 0.5, -0.25));
    let co2 = clamp(r.y * co - r.z * cg, -0.5, 0.5);
    let cg2 = clamp(r.z * co + r.y * cg, -0.5, 0.5);
    return max(vec3(y + co2 - cg2, y + cg2, y - co2 - cg2), vec3(0.0));
}

// How much of the sun a blade's foot and tip take (Studio's grass vertex
// shaders give each vertex exp2((-foot x a + b) x c), from three uniforms
// Rook can't read; measured from the side under the sun alone, a blade's
// foot draws about half as lit as its tip: Approximation).
const BLADE_FOOT_SUN: f32 = 0.5;
const BLADE_TIP_SUN: f32 = 1.0;

// Studs per repeat of water's normal frames (Approximation: Studio's
// ripples are 1 to 3 studs across, about two of the maps' bumps, and
// don't repeat within 43 studs).
const WATER_NORMAL_STUDS: f32 = 44.0;
// Frames per unit of water's phase (measured: the 25 frames come round
// every 20 units, 2 seconds at WaterWaveSpeed 10).
const WATER_FRAMES_PER_PHASE: f32 = 1.25;
// How much of the sky water reflects at WaterReflectance 1 looking
// straight down, rising to all of it along the surface by Schlick's
// Fresnel (measured in Studio 0.742 against the default skybox: 0.27 seen
// from above with a black WaterColor; against the sky it mirrors, 0.3 at
// 20 to 30 degrees, 0.69 at 10, 0.80 at 5 and 0.91 at 3).
const WATER_REFLECTS_FACING: f32 = 0.27;
// What water lets through of what's behind it, per unit of
// WaterTransparency (measured from above over gray and sand at 0, 0.3
// and 1: about a fifth at 1 in green and blue, half that in red; the
// depth, 3 to 10 studs, barely matters).
const WATER_PASSES: f32 = 0.2;
// Water's body takes more light than a part's face would (measured from
// above in white and in WaterColor: about 1.2 times; Approximation).
const WATER_BODY_LIGHT: f32 = 1.2;
// How steeply the normal frames ripple the surface's light and reflection
// (fitted: seen 1 stud above it, the reflection's contrast across the
// water is Studio's within 1.5 levels from 3 to 20 degrees down).
const WATER_RIPPLES: f32 = 0.5;
// The sun's glint takes the frames' ripples this much (unscaled by
// WATER_RIPPLES) and falls off at this power (fitted to how much of
// Studio's water is near white: 1.8% from above on the sun's side, 0.1%
// away from it, 5% along it towards the sun, none along it away from the
// sun; Rook 1.6%, 0.7%, 1.9% and none).
const WATER_GLINT_RIPPLES: f32 = 1.2;
const WATER_GLINT_POWER: f32 = 300.0;

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

// The width of a tile of blades (`rook_terrain::grass::TILE_STUDS`),
// placed at its corner.
const BLADE_TILE_STUDS: f32 = 32.0;

// Whether the blade numbered `n` (0 to 1) of the tile placed at `tile`
// is drawn, in the frame and in the shadow map alike. As Studio's grass
// shaders cull (`FFlagGrassGPUDensityCull`): one density for the whole
// draw, from how far it is, against a number per blade; a culled blade's
// vertices all go to one point, so it's never rasterized. Rook's density
// is 1 out to 60 studs from the tile's center, falling to 0.4 at the
// edge of where grass is drawn (observed: Studio's blades are as dense
// at 100 studs, sparser at 200; Approximation for the curve).
fn blade_drawn(tile: vec3<f32>, n: f32) -> bool {
    let center = tile.xz + vec2(BLADE_TILE_STUDS * 0.5);
    let dist = distance(center, g.camera_pos.xz);
    return n <= 1.0 - 0.6 * smoothstep(60.0, g.terrain.w, dist);
}

// Where a terrain vertex at `world` is drawn: water moved by the waves;
// a grass blade's vertex (`uv`: a number from 0 to 1 for its blade, its
// height above the blade's foot) swayed, leant into GlobalWind, and sunk
// into the ground towards the edge of where grass is drawn, as Studio's
// grass vertex shaders sink them by their distance.
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
        let grow = clamp((reach - dist) / (reach * 0.25), 0.0, 1.0);
        p.y -= (h + 0.1) * (1.0 - grow);
    }
    return p;
}

// A water normal frame's slope at `uv` (X in alpha, Y in green).
fn water_frame(uv: vec2<f32>, frame: i32, dx: vec2<f32>, dy: vec2<f32>) -> vec2<f32> {
    return textureSampleGrad(water_normals, image_sampler, uv, frame, dx, dy).ag * 2.0 - 1.0;
}

// The normal frames' slope across the plane of `t1` and `t2` at `p`,
// frame `f0` fading `k` of the way into the next, as a vector in it.
fn water_ripple(p: vec3<f32>, dp_x: vec3<f32>, dp_y: vec3<f32>, t1: vec3<f32>, t2: vec3<f32>, f0: f32, k: f32) -> vec3<f32> {
    let frames = f32(textureNumLayers(water_normals));
    let uv = vec2(dot(p, t1), dot(p, t2)) / WATER_NORMAL_STUDS;
    let dx = vec2(dot(dp_x, t1), dot(dp_x, t2)) / WATER_NORMAL_STUDS;
    let dy = vec2(dot(dp_y, t1), dot(dp_y, t2)) / WATER_NORMAL_STUDS;
    let s0 = water_frame(uv, i32(f0 % frames), dx, dy);
    let s1 = water_frame(uv, i32((f0 + 1.0) % frames), dx, dy);
    let s = mix(s0, s1, k);
    return t1 * s.x + t2 * s.y;
}

// Water's surface: WaterColor lit like a part's top on every face, the
// skybox reflected (WATER_REFLECTS_FACING of it looking down to all of it
// along the surface, times WaterReflectance), the sun's glint, and a
// little of what's behind it showing through (WATER_PASSES; observed: as much through its
// sides as from above).
// Its normal follows the waves, rippled by the normal frames, which play
// at WATER_FRAMES_PER_PHASE, each fading into the next (Inference: shots a
// third of a frame apart differ as much as the next third does), and stay
// in place (measured: they drift less than 0.07 studs per unit).
fn water_surface(in: VertexOut, dp_x: vec3<f32>, dp_y: vec3<f32>) -> vec4<f32> {
    let w = wave(in.world.xz);
    // The waves tilt the top; the sides, less as they turn away from up.
    let face = normalize(in.normal);
    var n = normalize(face + vec3(-w.y, 0.0, -w.z) * max(face.y, 0.0));
    let f = g.wind.w * WATER_FRAMES_PER_PHASE;
    let f0 = floor(f);
    // The frames lie on the three axes' planes, blended by how much the
    // surface faces each, top and sides alike (observed: Studio's sides
    // ripple as its top does, rounding over the edge without a seam).
    let k = f - f0;
    var weights = pow(abs(n), vec3(4.0));
    weights /= weights.x + weights.y + weights.z;
    let ripple = weights.x * water_ripple(in.world, dp_x, dp_y, vec3(0.0, 0.0, 1.0), vec3(0.0, 1.0, 0.0), f0, k)
        + weights.y * water_ripple(in.world, dp_x, dp_y, vec3(1.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0), f0, k)
        + weights.z * water_ripple(in.world, dp_x, dp_y, vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), f0, k);
    let strength = WATER_RIPPLES * clamp(g.terrain.y * 2.0 + 0.2, 0.2, 1.0);
    let glint_n = normalize(n + ripple * WATER_GLINT_RIPPLES);
    n = normalize(n + ripple * strength);
    let v = normalize(g.camera_pos.xyz - in.world);
    let fresnel = 0.02 + 0.98 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
    let up = vec3(0.0, 1.0, 0.0);
    // The body is lit as the top is on every face (measured: Studio's
    // sides away from the sun are as light as its top).
    let lit = normalize(vec3(0.0, 1.0, 0.0) + ripple * strength);
    let body = WATER_BODY_LIGHT * diffuse_light(albedo_units(in.color.rgb), in.world, lit, sun_light(in.world, lit));
    let reflected = clamp(in.material.x, 0.0, 1.0) * mix(WATER_REFLECTS_FACING, 1.0, fresnel);
    let r = reflect(-v, n);
    // Ripples can point a reflection below the horizon: keep it on the sky.
    let sky = light_units3(sky_toward(normalize(vec3(r.x, max(r.y, 0.02), r.z))) * g.fog_range.z);
    var c = mix(body, sky, reflected);
    if g.sun_dir.y > 0.0 {
        // The sun's glint, in light units: up to the sun's light on a face
        // square to it.
        let glint = pow(max(dot(reflect(-v, glint_n), g.sun_dir.xyz), 0.0), WATER_GLINT_POWER);
        c += glint * g.sun_color.rgb * sun_visibility(in.world, up);
    }
    let passes = (1.0 - in.color.a) * WATER_PASSES;
    return vec4(frame(fog_units(c, in.world)), 1.0 - passes);
}

// A grass blade's light, as Studio's grass shaders give it
// (`shaders_glsl3.pack`, 0.743): the blade's color times the ambient and
// sky light, plus the sun on it (`sun`: the share its foot or tip takes,
// interpolated, in `uv.y`) by (|n.l| + 0.9) / 1.9, so blades edge-on to
// the sun keep about half; and where Studio's high quality grass adds
// them, the light through a blade seen against the sun, (|n.l| / 2 +
// 0.5) cubed times the view's cosine to the sun, and a sharp highlight,
// both fading out over the last 40 studs of where grass is drawn. The
// blade is one triangle seen from both sides, so its normal's sign is
// whichever faces the sun. Shadowed through the shadow map, which blades
// cast into.
fn blade_surface(in: VertexOut, dp_x: vec3<f32>, dp_y: vec3<f32>) -> vec4<f32> {
    let albedo = albedo_units(in.color.rgb);
    let l = g.sun_dir.xyz;
    var n = normalize(cross(dp_x, dp_y));
    if dot(n, l) < 0.0 {
        n = -n;
    }
    let facing = abs(dot(n, l));
    let sun = in.uv.y;
    let shadow = sun_visibility(in.world, n);
    let lit = g.sun_color.rgb * shadow;
    var c = diffuse_light(albedo, in.world, vec3(0.0, 1.0, 0.0), lit * clamp((facing + 0.9) / 1.9, 0.0, 1.0) * sun);
    let to_camera = g.camera_pos.xyz - in.world;
    let distance = length(to_camera);
    let v = to_camera / max(distance, 1e-3);
    let fade = 1.0 - clamp((distance - g.terrain.w + 40.0) / 40.0, 0.0, 1.0);
    let half = normalize(l + v);
    let through = clamp(-dot(v, l) * fade * sun * pow(facing * 0.5 + 0.5, 3.0), 0.0, 1.0);
    let highlight = pow(abs(dot(half, n)), 12.0) * pow(clamp(half.y, 0.0, 1.0), 8.0) * fade * sun;
    c += (albedo * through + vec3(highlight)) * lit;
    return vec4(frame(fog_units(c, in.world)), 1.0);
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
