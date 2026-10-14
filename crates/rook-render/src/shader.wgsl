// Parts: one draw per (shape, image), one instance per part or image layer.
//
// Lighting is an Approximation of Roblox's look until it is tuned against
// Studio screenshots: Lambert sun light plus Ambient and an OutdoorAmbient
// term that fades on faces pointing down, then linear fog. Only the sun
// term is shadowed; Ambient and OutdoorAmbient light shadowed areas too,
// as they look in Roblox (Approximation).
//
// Images on faces follow `rook_types::NormalId` (the face frames come in
// `g.faces`, filled from it): a point belongs to its vertices' box face (the
// round primitives' grid patches, `mesh.rs`), or else to the face its normal
// is closest to, and its place on that face's image is measured in studs
// from the image's top-left corner, from its vertices' points on the box.

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
    let lo = v.i_box_min * v.i_size;
    let hi = v.i_box_max * v.i_size;
    var local = v.box_point * v.i_size;
    var normal = local_normal;
    var face = v.face;
    var center = (lo + hi) * 0.5;
    var size = abs(hi - lo);
    // Images and surfaces go by the part's own axes, not the turned
    // shape's: the turn takes the shape's (x, y, z) to the part's
    // (-y, x, z).
    if (v.i_flags & UPRIGHT) != 0u {
        local = vec3(-local.y, local.x, local.z);
        normal = vec3(-normal.y, normal.x, normal.z);
        center = vec3(-center.y, center.x, center.z);
        size = size.yxz;
        // The turned shape's faces in the part's axes (+X to +Y, +Y to -X).
        if face < 6u {
            face = array<u32, 6>(1u, 3u, 2u, 4u, 0u, 5u)[face];
        }
    }
    out.face = face;
    out.local = local;
    out.local_normal = normal;
    out.box_center = center;
    out.box_size = size;
    out.tile = v.i_tile;
    out.flags = v.i_flags;
    out.surfaces = v.i_flags >> 8u;
    out.reflectance = v.i_reflectance;
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

// The face a point belongs to: its vertices' box face, or else the one
// its normal is closest to.
fn face_at(in: VertexOut) -> u32 {
    if in.face < 6u {
        return in.face;
    }
    return face_of(in.local_normal);
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
    var albedo = in.color.rgb * textureSample(image, image_sampler, in.uv).rgb;
    let face = face_at(in);
    let tile = g.faces[0].w;
    let st = face_position(face, in.local, in.box_center, in.box_size).xy / tile;
    let dx = dpdx(st);
    let dy = dpdy(st);
    if in.surfaces != 0u {
        let pattern = (in.surfaces >> (face * 4u)) & 15u;
        if pattern != 0u {
            // The patterns are grey around 0.5: twice them leaves the
            // colour as it is between the studs.
            let s = textureSampleGrad(surface_atlas, image_sampler, st, i32(pattern - 1u), dx, dy);
            albedo *= s.rgb * 2.0;
        }
    }
    if (in.flags & NEON) != 0u {
        return vec4(fog(albedo, in.world), in.color.a);
    }
    var lit = shade(albedo, in.world, in.normal);
    // Reflectance: the part takes on its surroundings, here the skybox's
    // average colour (Approximation: Studio reflects the skybox itself; a
    // red part at 0.2 looks pink under the default blue sky, and a yellow
    // one at 0.3 olive under a black Sky; observed).
    if in.reflectance > 0.0 {
        lit = mix(lit, fog(srgb_to_linear(g.sky.rgb), in.world), in.reflectance);
    }
    return vec4(lit, in.color.a);
}

// Decals and Textures, blended over what's drawn.
@fragment
fn fs_layer(in: VertexOut) -> @location(0) vec4<f32> {
    let mode = (in.flags >> 2u) & 3u;
    let layer_face = (in.flags >> 4u) & 7u;
    let face = face_at(in);
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

// The skybox: a full-screen triangle per image (the instance is the image),
// at the far plane.
struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
    @location(1) @interpolate(flat) face: u32,
};

@vertex
fn vs_sky(@builtin(vertex_index) i: u32, @builtin(instance_index) face: u32) -> SkyOut {
    let xy = vec2(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var out: SkyOut;
    out.clip = vec4(xy, 0.0, 1.0);
    out.ndc = xy;
    out.face = face;
    return out;
}

// One skybox image: the pixels whose direction is closest to its face's,
// mapped straight onto the cube face (unlit and unfogged).
@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let p = g.sky_from_clip * vec4(in.ndc, 1.0, 1.0);
    let d = normalize(p.xyz / p.w);
    let dir = g.sky_faces[in.face * 3u].xyz;
    let right = g.sky_faces[in.face * 3u + 1u].xyz;
    let up = g.sky_faces[in.face * 3u + 2u].xyz;
    let along = dot(d, dir);
    let on_face = d / max(along, 1e-6);
    var uv = vec2(0.5 + 0.5 * dot(on_face, right), 0.5 - 0.5 * dot(on_face, up));
    // Images don't repeat: keep the sampler off the far edges.
    let half_texel = 0.5 / vec2<f32>(textureDimensions(image));
    uv = clamp(uv, half_texel, vec2(1.0) - half_texel);
    let texel = textureSample(image, image_sampler, uv);
    let a = abs(d);
    if along < max(a.x, max(a.y, a.z)) - 1e-6 {
        discard;
    }
    return vec4(texel.rgb, 1.0);
}
