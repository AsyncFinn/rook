// Bindings, instance layout and lighting shared by every pass. The
// renderer joins this file with parts.wgsl and sky.wgsl into one module.

struct Globals {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_dir: vec4<f32>,
    ambient: vec4<f32>,
    outdoor_ambient: vec4<f32>,
    // rgb fog colour, w = brightness
    fog_color: vec4<f32>,
    // x = fog start, y = fog end
    fog_range: vec4<f32>,
    // World to the shadow map's clip space (reverse Z).
    shadow_view_proj: mat4x4<f32>,
    // x = a texel in uv, y = normal offset in studs, z = depth bias,
    // w = how far shadows reach (0: no shadows this frame)
    shadow: vec4<f32>,
    // Each face's image right (even) and up (odd) in part axes, in
    // `NormalId` order; w = studs per surface pattern tile.
    faces: array<vec4<f32>, 12>,
    // What reflective parts take on: the skybox's average colour (sRGB);
    // w = EnvironmentSpecularScale.
    sky: vec4<f32>,
    // From clip space to directions from the camera (the skybox).
    sky_from_clip: mat4x4<f32>,
    // Each skybox image's direction, right and up (`SKYBOX_FACES`).
    sky_faces: array<vec4<f32>, 18>,
    // x = seconds, y = WaterWaveSize, z = WaterWaveSpeed, w = how far
    // grass is drawn.
    terrain: vec4<f32>,
    // GlobalWind (xyz).
    wind: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
// The surface patterns (studs, lattice, inlet, universal), one per layer.
@group(1) @binding(2) var surface_atlas: texture_2d_array<f32>;
// Linear, mipmapped, repeating: images and the surface patterns.
@group(1) @binding(3) var image_sampler: sampler;
// The batch's images (white, or a flat normal, when it has none): its
// image (a mesh texture, a decal or a skybox face), and its material's
// colour, normal, roughness (green) and metalness (red) maps.
@group(2) @binding(0) var image: texture_2d<f32>;
@group(2) @binding(1) var color_map: texture_2d<f32>;
@group(2) @binding(2) var normal_map: texture_2d<f32>;
@group(2) @binding(3) var roughness_map: texture_2d<f32>;
@group(2) @binding(4) var metalness_map: texture_2d<f32>;

// Instance flags.
const CAST: u32 = 1u;
const NEON: u32 = 2u;
// Bits 2-3: how a layer places its image.
const MODE_FACE: u32 = 1u;
const MODE_TILED: u32 = 2u;
const MODE_MESH_UV: u32 = 3u;
// The shape is turned onto the part's Y axis (a CylinderMesh).
const UPRIGHT: u32 = 128u;
// From bit 8, each face's surface pattern + 1 (0: none), 3 bits per face.
const SURFACES_SHIFT: u32 = 8u;
// The part has material textures, tiled at `tile.xy` studs.
const MAPS: u32 = 67108864u;
const GLASS: u32 = 134217728u;
const FORCE_FIELD: u32 = 268435456u;
// A wedge or corner wedge: its slopes take material textures in their plane.
const SLOPED: u32 = 536870912u;
// The material textures go through the mesh's UVs (a SurfaceAppearance),
// with its alpha mode in the surfaces' bits.
const MAPS_UV: u32 = 1073741824u;
// The vertices' own colours (in `face`'s upper bytes) replace the part's.
const VERTEX_COLORS: u32 = 2147483648u;
// A union vertex's face byte carries this bit (`mesh::UNION_FACE`).
const UNION_FACE: u32 = 64u;
// A terrain vertex's face byte (`mesh::TERRAIN_FACE`), plus 1 for textures
// on the axes' planes only, 2 for water, 4 for grass blades.
const TERRAIN_FACE: u32 = 128u;
const TERRAIN_WATER: u32 = 2u;
const TERRAIN_BLADES: u32 = 4u;
// Set on `VertexOut::face` for union vertices: tile from the face's centre.
const CENTRED: u32 = 256u;
const ALPHA_OVERLAY: u32 = 1u;
const ALPHA_TRANSPARENCY: u32 = 2u;
const ALPHA_TINT_MASK: u32 = 3u;
const PI: f32 = 3.14159265;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    // The point on the shape's box that images map from, and its face
    // (low byte; 255: none, found from the normal), with a union's vertex
    // colour in the bytes above.
    @location(3) box_point: vec3<f32>,
    @location(4) face: u32,
    @location(5) i_position: vec3<f32>,
    @location(6) i_right: vec3<f32>,
    @location(7) i_up: vec3<f32>,
    @location(8) i_back: vec3<f32>,
    @location(9) i_size: vec3<f32>,
    @location(10) i_color: vec4<f32>,
    // The mesh's bounds in its own units, so images on faces cover the
    // drawn mesh's box.
    @location(11) i_box_min: vec3<f32>,
    @location(12) i_box_max: vec3<f32>,
    // A Texture's studs per tile (xy) and offset (zw); a part's material
    // textures' studs per tile (xy).
    @location(13) i_tile: vec4<f32>,
    // CAST, NEON, the layer's mode (bits 2-3) and face (bits 4-6),
    // UPRIGHT (bit 7), and the surface patterns (SURFACES_SHIFT).
    @location(14) i_flags: u32,
    // Reflectance, roughness, metalness.
    @location(15) i_material: vec4<f32>,
};

struct VertexOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
    // Studs from the part's position, in its axes.
    @location(4) local: vec3<f32>,
    // The surface normal in the part's axes.
    @location(5) local_normal: vec3<f32>,
    @location(6) @interpolate(flat) box_center: vec3<f32>,
    @location(7) @interpolate(flat) box_size: vec3<f32>,
    @location(8) @interpolate(flat) tile: vec4<f32>,
    @location(9) @interpolate(flat) flags: u32,
    @location(10) @interpolate(flat) surfaces: u32,
    // Reflectance, roughness, metalness.
    @location(11) @interpolate(flat) material: vec4<f32>,
    // The box face the point belongs to (255: from the normal).
    @location(12) @interpolate(flat) face: u32,
};

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

fn instance_rotation(v: VertexIn) -> mat3x3<f32> {
    return mat3x3<f32>(v.i_right, v.i_up, v.i_back);
}

// Where a vertex of an instance is in the world (every pass).
fn world_position(v: VertexIn) -> vec3<f32> {
    return v.i_position + instance_rotation(v) * (v.position * v.i_size);
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
