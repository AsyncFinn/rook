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

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var world = world_position(v);
    let packed = v.face & 255u;
    if packed >= TERRAIN_FACE && packed != 255u {
        world = terrain_motion(world, packed, v.uv);
    }
    // Inverse-transpose of a rotation times a scale: rotate n / size.
    let local_normal = normalize(v.normal / max(abs(v.i_size), vec3(1e-4)) * sign(v.i_size));
    var out: VertexOut;
    out.clip = g.view_proj * vec4(world, 1.0);
    out.world = world;
    out.normal = normalize(instance_rotation(v) * local_normal);
    out.color = vec4(srgb_to_linear(v.i_color.rgb), v.i_color.a);
    if (v.i_flags.x & VERTEX_COLORS) != 0u {
        let c = vec3((v.face >> 8u) & 255u, (v.face >> 16u) & 255u, v.face >> 24u);
        out.color = vec4(srgb_to_linear(vec3<f32>(c) / 255.0), v.i_color.a);
    }
    out.uv = v.uv;
    let lo = v.i_box_min * v.i_size;
    let hi = v.i_box_max * v.i_size;
    var local = v.box_point * v.i_size;
    var normal = local_normal;
    // Union vertices (bit 6 of their face) tile from their face's centre.
    let centred = packed != 255u && (packed & UNION_FACE) != 0u;
    var face = select(packed, packed & 7u, centred);
    var center = (lo + hi) * 0.5;
    var size = abs(hi - lo);
    // Images and surfaces go by the part's own axes, not the turned
    // shape's: the turn takes the shape's (x, y, z) to the part's
    // (-y, x, z).
    if (v.i_flags.x & UPRIGHT) != 0u {
        local = vec3(-local.y, local.x, local.z);
        normal = vec3(-normal.y, normal.x, normal.z);
        center = vec3(-center.y, center.x, center.z);
        size = size.yxz;
        // The turned shape's faces in the part's axes (+X to +Y, +Y to -X).
        if face < 6u {
            face = array<u32, 6>(1u, 3u, 2u, 4u, 0u, 5u)[face];
        }
    }
    out.face = face | select(0u, CENTRED, centred);
    out.local = local;
    out.local_normal = normal;
    out.box_center = center;
    out.box_size = size;
    out.tile = v.i_tile;
    out.flags = v.i_flags.x;
    out.surfaces = v.i_flags.x >> SURFACES_SHIFT;
    out.material = v.i_material;
    out.images = v.i_flags.yzw;
    return out;
}

// The shadow map: depth only, from the sun.
@vertex
fn vs_shadow(v: ShadowIn) -> @builtin(position) vec4<f32> {
    if (v.i_flags.x & CAST) == 0u {
        // Outside the depth range, so every triangle is clipped away.
        return vec4(0.0, 0.0, -1.0, 1.0);
    }
    let rotation = mat3x3<f32>(v.i_right, v.i_up, v.i_back);
    return g.shadow_view_proj * vec4(placed(v.position, v.i_position, rotation, v.i_size), 1.0);
}

// The face a point belongs to: its vertices' box face, or else the one
// its normal is closest to.
fn face_at(in: VertexOut) -> u32 {
    let face = in.face & 255u;
    if face < 6u {
        return face;
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

// Where tiling starts on a face: its image's top-left corner, or on a
// union its centre (Studio tiles Textures on unions through the union's
// UVs, planar and in studs from its centre; measured on a resized union,
// where the tiles sat a quarter stud from where corner tiling puts them).
// `at` is in studs from the top-left corner of a face of size `extent`.
fn tiling_position(in: VertexOut, at: vec2<f32>, extent: vec2<f32>) -> vec2<f32> {
    return select(at, at - extent * 0.5, (in.face & CENTRED) != 0u);
}

// Studs from the top-left corner of `face`'s image, and the face's size.
fn face_position(face: u32, local: vec3<f32>, center: vec3<f32>, size: vec3<f32>) -> vec4<f32> {
    let right = g.faces[face * 2u].xyz;
    let up = g.faces[face * 2u + 1u].xyz;
    let extent = vec2(dot(abs(right), size), dot(abs(up), size));
    let p = local - center;
    return vec4(dot(p, right) + extent.x * 0.5, extent.y * 0.5 - dot(p, up), extent);
}

// A normal map's normal `t` (X right and Y up the image, Z out) turned
// into the world from the surface's normal `n`, using how the world
// position `p` and the map's coordinates change across the pixel (a
// cotangent frame, so no tangents need storing). The map's coordinates
// grow down the image.
fn perturb(n: vec3<f32>, t: vec3<f32>, dp_x: vec3<f32>, dp_y: vec3<f32>, duv_x: vec2<f32>, duv_y: vec2<f32>) -> vec3<f32> {
    let n0 = normalize(n);
    let y_perp = cross(dp_y, n0);
    let x_perp = cross(n0, dp_x);
    let tangent = y_perp * duv_x.x + x_perp * duv_y.x;
    let bitangent = y_perp * duv_x.y + x_perp * duv_y.y;
    let scale = inverseSqrt(max(max(dot(tangent, tangent), dot(bitangent, bitangent)), 1e-20));
    return normalize(tangent * scale * t.x - bitangent * scale * t.y + n0 * t.z);
}

// Where a point is on its material's textures, in studs from the
// top-left corner of its face: as images go (`at`), except on wedges,
// whose faces use the face their own normal is closest to and lengths
// measured in their plane, so a long, low wedge's slope tiles like a top
// instead of stretching the Front face it belongs to for images over its
// whole length (reported on a large modern place; Inference that Studio measures
// along the slope).
fn material_position(in: VertexOut, face: u32, at: vec2<f32>) -> vec2<f32> {
    if (in.flags & SLOPED) == 0u {
        return at;
    }
    let n = normalize(in.local_normal);
    let plane_face = face_of(n);
    let right = g.faces[plane_face * 2u].xyz;
    let up = g.faces[plane_face * 2u + 1u].xyz;
    let r = normalize(right - n * dot(n, right));
    let u = normalize(up - n * dot(n, up));
    let extent = vec2(dot(abs(right), in.box_size), dot(abs(up), in.box_size))
        / max(vec2(abs(dot(right, r)), abs(dot(up, u))), vec2(1e-3));
    let p = in.local - in.box_center;
    return vec2(dot(p, r) + extent.x * 0.5, extent.y * 0.5 - dot(p, u));
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

// Neon glows past its Color: in linear light, 18 times its square, up to
// full brightness (fitted in Studio 0.742 to eight colours: grey 128 drew
// 237, 64 drew 60, (196, 40, 28) drew (255, 20, 7)).
fn neon(c: vec3<f32>) -> vec3<f32> {
    return min(c * c * 18.0, vec3(1.0));
}

// Parts and their mesh textures. A texture's alpha is ignored (observed).
@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    var albedo = in.color.rgb * sample_image(in.images, in.uv).rgb;
    let face = face_at(in);
    let placed = face_position(face, in.local, in.box_center, in.box_size);
    let at = placed.xy;
    let tile = g.faces[0].w;
    let st = at / tile;
    let dx = dpdx(st);
    let dy = dpdy(st);
    // The material's textures tile like a Texture, from each face's
    // top-left corner (measured: unrotated, unmirrored).
    let on_uv = (in.flags & MAPS_UV) != 0u;
    let tiled = tiling_position(in, material_position(in, face, at), placed.zw);
    var uv = select(tiled / max(in.tile.xy, vec2(1e-3)), in.uv, on_uv);
    let packed = in.face & 255u;
    let terrain = packed >= TERRAIN_FACE && packed != 255u;
    if terrain {
        uv = terrain_position(in.world, in.normal, (packed & 1u) != 0u) / max(in.tile.xy, vec2(1e-3));
    }
    let duv_x = dpdx(uv);
    let duv_y = dpdy(uv);
    let dp_x = dpdx(in.world);
    let dp_y = dpdy(in.world);
    if terrain && (packed & TERRAIN_WATER) != 0u {
        return water_surface(in, dp_x, dp_y);
    }
    var normal = in.normal;
    var roughness = in.material.y;
    var metalness = in.material.z;
    var alpha = surface_alpha(in);
    if on_uv {
        let c = sample_map(in.images, COLOR_MAP, uv, duv_x, duv_y);
        let a = surface_appearance(in, c);
        albedo = a.rgb;
        alpha *= a.a;
    } else if terrain && (in.flags & MAPS) != 0u {
        // Terrain's colour maps are tint masks (their packs' `alphamode`
        // 2, TintMask): where they're opaque, Studio keeps only the
        // texel's luma, times the material's tint for its colour
        // (`TerrainMaterial::tint`, the instance colour).
        let c = sample_map(in.images, COLOR_MAP, uv, duv_x, duv_y);
        let luma = dot(c.rgb, vec3(0.25, 0.5, 0.25));
        albedo = mix(c.rgb, luma * in.color.rgb, c.a);
    } else if (in.flags & MAPS) != 0u {
        albedo *= sample_map(in.images, COLOR_MAP, uv, duv_x, duv_y).rgb;
    }
    if (in.flags & MAPS) != 0u {
        roughness *= sample_map(in.images, ROUGHNESS_MAP, uv, duv_x, duv_y).g;
        metalness *= sample_map(in.images, METALNESS_MAP, uv, duv_x, duv_y).r;
        let t = sample_map(in.images, NORMAL_MAP, uv, duv_x, duv_y).xyz * 2.0 - 1.0;
        normal = perturb(in.normal, t, dp_x, dp_y, duv_x, duv_y);
    }
    if in.surfaces != 0u && (in.flags & MAPS_UV) == 0u {
        let pattern = (in.surfaces >> (face * 3u)) & 7u;
        if pattern != 0u {
            // The patterns are grey around 0.5: twice them leaves the
            // colour as it is between the studs.
            let s = textureSampleGrad(surface_atlas, image_sampler, st, i32(pattern - 1u), dx, dy);
            albedo *= s.rgb * 2.0;
        }
    }
    if (in.flags & NEON) != 0u {
        return vec4(fog(neon(albedo), in.world), in.color.a);
    }
    var lit = shade_pbr(albedo, in.world, normal, roughness, metalness);
    // Reflectance: the part takes on its surroundings, here the skybox's
    // average colour (Approximation: Studio reflects the skybox itself; a
    // red part at 0.2 looks pink under the default blue sky, and a yellow
    // one at 0.3 olive under a black Sky; observed).
    let reflectance = in.material.x;
    if reflectance > 0.0 {
        lit = mix(lit, fog(srgb_to_linear(g.sky.rgb), in.world), reflectance);
    }
    return vec4(lit, alpha);
}

// A SurfaceAppearance's colour (linear) and opacity from its colour map
// `c` (creator docs, checked in Studio 0.742 with a map whose see-through
// pixels are white): the map times its tint (`Color`), where Overlay shows
// the part's Color through the map's see-through pixels, Transparency
// makes them see-through, TintMask tints only where the map is opaque,
// and Opaque ignores the alpha. The part's Color counts only for Overlay.
fn surface_appearance(in: VertexOut, c: vec4<f32>) -> vec4<f32> {
    let tint = srgb_to_linear(vec3(in.tile.z, in.tile.w, in.material.w));
    let mode = in.surfaces & 3u;
    if mode == ALPHA_OVERLAY {
        return vec4(mix(in.color.rgb, c.rgb * tint, c.a), 1.0);
    }
    if mode == ALPHA_TRANSPARENCY {
        return vec4(c.rgb * tint, c.a);
    }
    if mode == ALPHA_TINT_MASK {
        return vec4(c.rgb * mix(vec3(1.0), tint, c.a), 1.0);
    }
    return vec4(c.rgb * tint, 1.0);
}

// How much of a part's own surface shows. See-through Glass mostly
// filters what's behind it (`fs_glass_filter`): its surface fades out by
// Transparency 0.5 (observed: red Glass at 0.5 over blue drew (16, 4, 6),
// all but the blue filtered; Approximation between).
fn surface_alpha(in: VertexOut) -> f32 {
    if (in.flags & GLASS) != 0u {
        let transparency = 1.0 - in.color.a;
        return clamp(1.0 - 2.0 * transparency, 0.0, 1.0);
    }
    return in.color.a;
}

// See-through Glass: what's behind is multiplied by the Glass's colour,
// fully up to Transparency about 0.65 and less beyond (observed: red Glass
// at 0.5 left almost nothing of a blue wall, at 0.8 turned yellow orange;
// fitted, Approximation).
@fragment
fn fs_glass_filter(in: VertexOut) -> @location(0) vec4<f32> {
    let strength = clamp(2.9 * in.color.a, 0.0, 1.0);
    return vec4(mix(vec3(1.0), in.color.rgb, strength), 1.0);
}

// ForceField: its colour added over what's behind, faint where it faces
// the camera and strong towards its outline, less as Transparency rises
// (observed on blocks and balls: face-on a red one added about 0.014 to
// the red behind it, at grazing angles 0.18; Approximation). Studio's
// moving pattern isn't drawn.
@fragment
fn fs_force_field(in: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let v = normalize(g.camera_pos.xyz - in.world);
    let rim = pow(1.0 - abs(dot(n, v)), 3.0);
    return vec4(in.color.rgb * (0.025 + 0.6 * rim) * in.color.a, 0.0);
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
        uv = (tiling_position(in, at.xy, at.zw) + in.tile.zw) / in.tile.xy;
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
        let half_texel = 0.5 / vec2<f32>(image_size(in.images));
        uv = clamp(uv, half_texel, vec2(1.0) - half_texel);
    }
    let texel = sample_map(in.images, IMAGE, uv, dx, dy);
    let alpha = texel.a * in.color.a;
    return vec4(shade(texel.rgb * in.color.rgb, in.world, in.normal), alpha);
}
