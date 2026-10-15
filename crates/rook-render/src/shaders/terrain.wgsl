// Terrain's moving surfaces: water's waves and grass blades' sway
// (docs/behavior/terrain.md). How Studio shapes its waves and sways its
// blades isn't known: these are Approximations of how they look.

// The waves at `p` (world X and Z): two sines across the water, together
// up to WaterWaveSize studs above and below it, moving faster with
// WaterWaveSpeed. Returns the height and its slope along X and Z. Observed
// in Studio 0.742: the waves move the surface itself, at WaterWaveSize 1
// by about a stud, in humps some 50 to 60 studs long.
fn wave(p: vec2<f32>) -> vec3<f32> {
    let size = g.terrain.y;
    let t = g.terrain.x * g.terrain.z * 0.1;
    // Wave vectors in radians per stud: wavelengths of 60 and 45 studs.
    let a = vec2(0.095, 0.045);
    let b = vec2(-0.05, 0.13);
    let pa = dot(p, a) + t;
    let pb = dot(p, b) + t * 0.8;
    let h = size * 0.5 * (sin(pa) + sin(pb));
    let d = size * 0.5 * (cos(pa) * a + cos(pb) * b);
    return vec3(h, d);
}

// Where a terrain vertex at `world` is drawn: water moved by the waves; a
// grass blade's vertex (`uv`: a number from 0 to 1 for its blade, its
// height above the blade's foot) swayed, bent by GlobalWind, and sunk into
// the ground as blades thin out from 60 studs away (observed: Studio's
// blades are as dense at 100 studs, sparser at 200) and towards the edge
// of where grass is drawn.
fn terrain_motion(world: vec3<f32>, packed: u32, uv: vec2<f32>) -> vec3<f32> {
    var p = world;
    if (packed & TERRAIN_WATER) != 0u {
        p.y += wave(p.xz).x;
    }
    if (packed & TERRAIN_BLADES) != 0u {
        let h = uv.y;
        let wind = g.wind.xz;
        let speed = length(wind);
        let dir = select(vec2(0.8, 0.6), wind / max(speed, 1e-3), speed > 1e-3);
        let t = g.terrain.x * (1.5 + speed * 0.1);
        let sway = sin(t + uv.x * 6.2831853 + dot(p.xz, vec2(0.15, 0.1))) * 0.08
            + min(speed * 0.02, 0.5);
        p += vec3(dir.x, 0.0, dir.y) * sway * h * h * 0.25;
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

// Water's surface: WaterColor lit from above, the sky reflected (half of
// WaterReflectance looking down, all of it at grazing angles, by
// Schlick's Fresnel), the sun's glint, and more opaque at grazing angles.
// Its normal follows the waves, rippled by the install's water normal map
// (DXT5nm: X in alpha, Y in green) scrolled two ways.
fn water_surface(in: VertexOut, dp_x: vec3<f32>, dp_y: vec3<f32>) -> vec4<f32> {
    let w = wave(in.world.xz);
    var n = normalize(vec3(-w.y, 1.0, -w.z));
    if in.normal.y < 0.5 {
        // Water's sides don't wave.
        n = normalize(in.normal);
    }
    if (in.flags & MAPS) != 0u {
        let t = g.terrain.x * g.terrain.z * 0.004;
        let scale = vec2(1.0 / 24.0, 1.0 / 15.0);
        let uv1 = in.world.xz * scale.x + vec2(t, t * 0.4);
        let uv2 = in.world.xz * scale.y + vec2(-t * 0.6, t);
        let s1 = textureSampleGrad(normal_map, image_sampler, uv1, dp_x.xz * scale.x, dp_y.xz * scale.x).ag * 2.0 - 1.0;
        let s2 = textureSampleGrad(normal_map, image_sampler, uv2, dp_x.xz * scale.y, dp_y.xz * scale.y).ag * 2.0 - 1.0;
        let strength = clamp(g.terrain.y * 2.0 + 0.2, 0.2, 1.0) * 0.5 * max(n.y, 0.0);
        n = normalize(n + vec3(s1.x + s2.x, 0.0, s1.y + s2.y) * strength);
    }
    let v = normalize(g.camera_pos.xyz - in.world);
    let fresnel = 0.02 + 0.98 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
    let up = vec3(0.0, 1.0, 0.0);
    let body = in.color.rgb * (ambient_light(up) + vec3(sun_light(in.world, up)));
    let reflected = clamp(in.material.x * mix(0.5, 1.0, fresnel), 0.0, 1.0);
    var c = mix(body, srgb_to_linear(g.sky.rgb), reflected);
    if g.sun_dir.y > 0.0 {
        let glint = pow(max(dot(reflect(-v, n), g.sun_dir.xyz), 0.0), 300.0);
        c += vec3(glint * g.fog_color.w * 2.0 * sun_visibility(in.world, up));
    }
    return vec4(fog(c, in.world), mix(in.color.a, 1.0, fresnel));
}
