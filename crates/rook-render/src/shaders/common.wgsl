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
    // What reflective parts take on: the skybox's average colour (sRGB).
    sky: vec4<f32>,
    // From clip space to directions from the camera (the skybox).
    sky_from_clip: mat4x4<f32>,
    // Each skybox image's direction, right and up (`SKYBOX_FACES`).
    sky_faces: array<vec4<f32>, 18>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
// The surface patterns (studs, lattice, inlet, universal), one per layer.
@group(1) @binding(2) var surface_atlas: texture_2d_array<f32>;
// Linear, mipmapped, repeating: images and the surface patterns.
@group(1) @binding(3) var image_sampler: sampler;
// The batch's image: a mesh texture or a decal (white when none).
@group(2) @binding(0) var image: texture_2d<f32>;

// Instance flags.
const CAST: u32 = 1u;
const NEON: u32 = 2u;
// Bits 2-3: how a layer places its image.
const MODE_FACE: u32 = 1u;
const MODE_TILED: u32 = 2u;
const MODE_MESH_UV: u32 = 3u;
// The shape is turned onto the part's Y axis (a CylinderMesh).
const UPRIGHT: u32 = 128u;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    // The point on the shape's box that images map from, and its face
    // (255: none, found from the normal).
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
    // A Texture's studs per tile (xy) and offset (zw).
    @location(13) i_tile: vec4<f32>,
    // CAST, NEON, the layer's mode (bits 2-3) and face (bits 4-6),
    // UPRIGHT (bit 7), and from bit 8 each face's surface pattern + 1 (0:
    // none), 4 bits per face.
    @location(14) i_flags: u32,
    @location(15) i_reflectance: f32,
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
    @location(11) @interpolate(flat) reflectance: f32,
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

// Sun, ambient and fog on a linear albedo.
fn shade(albedo: vec3<f32>, world: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let n = normalize(normal);
    var sun = max(dot(n, g.sun_dir.xyz), 0.0) * g.fog_color.w * 0.5;
    if sun > 0.0 {
        sun *= sun_visibility(world, n);
    }
    let sky = 0.5 + 0.5 * n.y;
    let ambient = srgb_to_linear(g.ambient.rgb) + srgb_to_linear(g.outdoor_ambient.rgb) * sky;
    return fog(albedo * (ambient + vec3(sun)), world);
}

fn fog(color: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let dist = distance(world, g.camera_pos.xyz);
    let f = clamp((dist - g.fog_range.x) / max(g.fog_range.y - g.fog_range.x, 1e-3), 0.0, 1.0);
    return mix(color, srgb_to_linear(g.fog_color.rgb), f);
}
