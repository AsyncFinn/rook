// Parts: one draw per (shape, image), one instance per part or image layer.
//
// Lighting is an Approximation of Roblox's look until it is tuned against
// Studio screenshots: Lambert sun light plus Ambient and an OutdoorAmbient
// term that fades on faces pointing down, then linear fog. Only the sun
// term is shadowed; Ambient and OutdoorAmbient light shadowed areas too,
// as they look in Roblox (Approximation).
//
// Images on faces follow `rook_types::NormalId` (the face frames come in
// `g.faces`, filled from it): a point belongs to the face its normal is
// closest to, and its place on that face's image is measured in studs from
// the image's top-left corner.

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
    // `NormalId` order.
    faces: array<vec4<f32>, 12>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
// Linear, mipmapped, repeating: images.
@group(1) @binding(3) var image_sampler: sampler;
// The batch's image: a mesh texture or a decal (white when none).
@group(2) @binding(0) var image: texture_2d<f32>;

// Instance flags.
const CAST: u32 = 1u;
// Bits 2-3: how a layer places its image.
const MODE_FACE: u32 = 1u;
const MODE_TILED: u32 = 2u;
const MODE_MESH_UV: u32 = 3u;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) i_position: vec3<f32>,
    @location(4) i_right: vec3<f32>,
    @location(5) i_up: vec3<f32>,
    @location(6) i_back: vec3<f32>,
    @location(7) i_size: vec3<f32>,
    @location(8) i_color: vec4<f32>,
    // The mesh's bounds in its own units, so images on faces cover the
    // drawn mesh's box.
    @location(9) i_box_min: vec3<f32>,
    @location(10) i_box_max: vec3<f32>,
    // A Texture's studs per tile (xy) and offset (zw).
    @location(11) i_tile: vec4<f32>,
    // CAST, the layer's mode (bits 2-3) and face (bits 4-6).
    @location(12) i_flags: u32,
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

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    let world = world_position(v);
    // Inverse-transpose of a rotation times a scale: rotate n / size.
    let local_normal = normalize(v.normal / max(abs(v.i_size), vec3(1e-4)) * sign(v.i_size));
    var out: VertexOut;
    out.clip = g.view_proj * vec4(world, 1.0);
    out.world = world;
    out.normal = normalize(instance_rotation(v) * local_normal);
    out.color = vec4(srgb_to_linear(v.i_color.rgb), v.i_color.a);
    out.uv = v.uv;
    out.local = v.position * v.i_size;
    out.local_normal = local_normal;
    let lo = v.i_box_min * v.i_size;
    let hi = v.i_box_max * v.i_size;
    out.box_center = (lo + hi) * 0.5;
    out.box_size = abs(hi - lo);
    out.tile = v.i_tile;
    out.flags = v.i_flags;
    return out;
}

// The shadow map: depth only, from the sun.
@vertex
fn vs_shadow(v: VertexIn) -> @builtin(position) vec4<f32> {
    if (v.i_flags & CAST) == 0u {
        // Outside the depth range, so every triangle is clipped away.
        return vec4(0.0, 0.0, -1.0, 1.0);
    }
    return g.shadow_view_proj * vec4(world_position(v), 1.0);
}

// The face (`NormalId` index) a surface with part-space normal `n` shows:
// the closest axis, Z before X before Y on a tie (`NormalId::of_normal`).
fn face_of(n: vec3<f32>) -> u32 {
    let a = abs(n);
    if a.z >= a.x && a.z >= a.y {
        return select(5u, 2u, n.z >= 0.0);
    }
    if a.x >= a.y {
        return select(3u, 0u, n.x >= 0.0);
    }
    return select(4u, 1u, n.y >= 0.0);
}

// Studs from the top-left corner of `face`'s image, and the face's size.
fn face_position(face: u32, local: vec3<f32>, center: vec3<f32>, size: vec3<f32>) -> vec4<f32> {
    let right = g.faces[face * 2u].xyz;
    let up = g.faces[face * 2u + 1u].xyz;
    let extent = vec2(dot(abs(right), size), dot(abs(up), size));
    let p = local - center;
    return vec4(dot(p, right) + extent.x * 0.5, extent.y * 0.5 - dot(p, up), extent);
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

// Parts and their mesh textures. A texture's alpha is ignored (observed).
@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let albedo = in.color.rgb * textureSample(image, image_sampler, in.uv).rgb;
    return vec4(shade(albedo, in.world, in.normal), in.color.a);
}

// Decals and Textures, blended over what's drawn.
@fragment
fn fs_layer(in: VertexOut) -> @location(0) vec4<f32> {
    let mode = (in.flags >> 2u) & 3u;
    let layer_face = (in.flags >> 4u) & 7u;
    let face = face_of(in.local_normal);
    let at = face_position(layer_face, in.local, in.box_center, in.box_size);
    var uv = in.uv;
    if mode == MODE_FACE {
        uv = at.xy / at.zw;
    } else if mode == MODE_TILED {
        uv = (at.xy + in.tile.zw) / in.tile.xy;
    }
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    if mode != MODE_MESH_UV && face != layer_face {
        discard;
    }
    if mode == MODE_FACE && (any(uv < vec2(0.0)) || any(uv > vec2(1.0))) {
        discard;
    }
    if mode != MODE_TILED {
        // A decal doesn't repeat: keep the repeating sampler off the far
        // edges (a head's (0, 0) UVs show the image's top-left texel,
        // observed).
        let half_texel = 0.5 / vec2<f32>(textureDimensions(image));
        uv = clamp(uv, half_texel, vec2(1.0) - half_texel);
    }
    let texel = textureSampleGrad(image, image_sampler, uv, dx, dy);
    let alpha = texel.a * in.color.a;
    return vec4(shade(texel.rgb * in.color.rgb, in.world, in.normal), alpha);
}
