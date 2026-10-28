// Terrain's own looks: its textures recolored by MaterialColors
// (docs/behavior/terrain/materials.md#colors), water's waves and normal
// frames, and grass blades' light, sway and lean
// (docs/behavior/terrain/grass.md). What Studio 0.742 shows was measured;
// the shapes chosen to match it are Approximations.

import package::common::{
    g, image_sampler, water_normals, VertexOut, TERRAIN_WATER, TERRAIN_BLADES,
    TERRAIN_DRAWN_SHIFT, terrain_taps,
};
import package::images::sky_toward;
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

// Water's normal frames as Studio's smooth water shaders lay them
// (`shaders_glsl3.pack`, 0.744): 20 studs a repeat on the plane the
// surface faces most, each vertex shifting them by a random number in
// tenths of a repeat, blended across the triangle (Rook: per point of the
// 4-stud lattice, as its terrain textures, `terrain_taps`).
const WATER_NORMAL_STUDS: f32 = 20.0;
const WATER_NORMAL_SHIFT: f32 = 0.1;
// Frames per unit of water's phase (measured: the 25 frames come round
// every 20 units, 2 seconds at WaterWaveSpeed 10).
const WATER_FRAMES_PER_PHASE: f32 = 1.25;
// What water lets through of what's behind it, per unit of
// WaterTransparency (measured from above over gray and sand at 0, 0.3
// and 1: about a fifth at 1 in green and blue, half that in red; the
// depth, 3 to 10 studs, barely matters).
const WATER_PASSES: f32 = 0.2;
// Water lets less through farther off, none from this many studs (Studio's
// blended water shader: opacity rises with the view depth times 0.004).
const WATER_OPAQUE_STUDS: f32 = 250.0;
// Water's body takes more light than a part's face would (measured from
// above in white and in WaterColor: about 1.2 times; Approximation).
const WATER_BODY_LIGHT: f32 = 1.2;

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

// A water normal frame at `uv`: its slope (X in alpha, Y in green) and
// roughness (red).
fn water_frame(uv: vec2<f32>, frame: i32, dx: vec2<f32>, dy: vec2<f32>) -> vec3<f32> {
    let t = textureSampleGrad(water_normals, image_sampler, uv, frame, dx, dy);
    return vec3(t.ag * 2.0 - 1.0, t.r);
}

// The surface's normal `n` rippled by the normal frames at `p`, frame `f0`
// fading `k` of the way into the next, with their roughness in `w`: the
// frames' whole slope across the plane `n` faces most, as Studio's shader
// takes it.
fn water_ripple(p: vec3<f32>, dp_x: vec3<f32>, dp_y: vec3<f32>, n: vec3<f32>, f0: f32, k: f32) -> vec4<f32> {
    let frames = f32(textureNumLayers(water_normals));
    let taps = terrain_taps(terrain_position(p, n, false), WATER_NORMAL_STUDS, WATER_NORMAL_SHIFT, 0.0);
    let dx = terrain_position(dp_x, n, false) / WATER_NORMAL_STUDS;
    let dy = terrain_position(dp_y, n, false) / WATER_NORMAL_STUDS;
    var s = vec3(0.0);
    for (var i = 0u; i < taps.count; i++) {
        let a = water_frame(taps.uv[i], i32(f0 % frames), dx, dy);
        let b = water_frame(taps.uv[i], i32((f0 + 1.0) % frames), dx, dy);
        s += taps.weight[i] * mix(a, b, k);
    }
    // The plane's axes in the world (its coordinates are linear in the
    // position).
    let x = terrain_position(vec3(1.0, 0.0, 0.0), n, false);
    let y = terrain_position(vec3(0.0, 1.0, 0.0), n, false);
    let z = terrain_position(vec3(0.0, 0.0, 1.0), n, false);
    let up = sqrt(clamp(1.0 - dot(s.xy, s.xy), 0.0, 1.0));
    let rippled = vec3(x.x, y.x, z.x) * s.x + vec3(x.y, y.y, z.y) * s.y + n * up;
    return vec4(normalize(rippled), s.z);
}

// Water's surface, as Studio's smooth water shaders draw it: WaterColor
// lit like a part's top on every face, mixed towards the skybox it
// reflects by WaterReflectance × (clamp(0.78 - 2.5 |n.v|) + 0.3), the
// normal there rippled a quarter of the way to the frames' (this fits the
// reflection measured at 3 to 30 degrees down: 0.91 to 0.3), with the
// sun's GGX glint off the frames' full ripples, their red channel the
// roughness, and a little of what's behind showing through (WATER_PASSES),
// less with distance. The shaders have no side: seen from below, the same
// terms reflect the sky's lower half, so the underside draws pale and
// rippled. The frames play at WATER_FRAMES_PER_PHASE, each fading into the
// next (the shader crossfades two), and stay in place (measured: they
// drift less than 0.07 studs per unit).
fn water_surface(in: VertexOut, dp_x: vec3<f32>, dp_y: vec3<f32>) -> vec4<f32> {
    let w = wave(in.world.xz);
    // The waves tilt the top; the sides, less as they turn away from up.
    let face = normalize(in.normal);
    let n = normalize(face + vec3(-w.y, 0.0, -w.z) * max(face.y, 0.0));
    let f = g.wind.w * WATER_FRAMES_PER_PHASE;
    let f0 = floor(f);
    let ripple = water_ripple(in.world, dp_x, dp_y, n, f0, f - f0);
    let rippled = ripple.xyz;
    let to_camera = g.camera_pos.xyz - in.world;
    let dist = length(to_camera);
    let v = to_camera / max(dist, 1e-3);
    let up = vec3(0.0, 1.0, 0.0);
    // The body is lit as the top is on every face (measured: Studio's
    // sides away from the sun are as light as its top).
    let body = WATER_BODY_LIGHT * diffuse_light(albedo_units(in.color.rgb), in.world, up, sun_light(in.world, up));
    let mirror = mix(n, rippled, 0.25);
    let reflected = clamp(in.material.x, 0.0, 1.0) * (clamp(0.78 - 2.5 * abs(dot(mirror, v)), 0.0, 1.0) + 0.3);
    let sky = light_units3(sky_toward(normalize(reflect(-v, mirror))) * g.fog_range.z);
    var c = mix(body, sky, reflected);
    if g.sun_dir.y > 0.0 {
        let l = g.sun_dir.xyz;
        let h = normalize(v + l);
        let rough = 0.089 + 0.911 * ripple.w;
        let a2 = rough * rough * rough * rough;
        let nh = max(0.001, dot(rippled, h));
        let lh = dot(l, h);
        let f5 = pow(1.0 - lh, 5.0);
        let fresnel = f5 + 0.02 * (1.0 - f5);
        let d = (nh * a2 - nh) * nh + 1.0;
        let glint = fresnel * (a2 + a2 * a2) / (d * d * (lh * 3.0 + 0.5) * (nh * 0.75 + 0.25));
        c += glint * clamp(dot(rippled, l), 0.0, 1.0) * g.sun_color.rgb * sun_visibility(in.world, up);
    }
    let passes = (1.0 - in.color.a) * WATER_PASSES * (1.0 - clamp(dist / WATER_OPAQUE_STUDS, 0.0, 1.0));
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

// A terrain seam fragment's share of its draw's layer: the barycentric
// weights of the triangle's corners that hold its layer. Studio's terrain
// shaders mix a seam's three layers by these plain weights (read from its
// GLSL), so seams are linear ramps a cell wide whatever the textures. The
// draws of a triangle add up to the whole.
fn terrain_share(in: VertexOut) -> f32 {
    let drawn = in.face >> TERRAIN_DRAWN_SHIFT;
    let b = vec3(in.uv.x, in.uv.y, 1.0 - in.uv.x - in.uv.y);
    let mine = vec3(f32(drawn & 1u), f32((drawn >> 1u) & 1u), f32((drawn >> 2u) & 1u));
    return dot(b, mine);
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
