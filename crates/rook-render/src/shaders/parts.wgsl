// Parts: one draw per (shape, image), one instance per part or image layer.
//
// Lighting comes from `lighting.wgsl`: Lambert sun light, shadowed through
// the shadow map, plus an ambient that is the brighter of Ambient and
// OutdoorAmbient on every face, unshadowed and never occluded from the sky
// (measured in Studio 0.742, docs/behavior/lighting/ambient.md), local lights, then
// linear fog.
//
// Images on faces follow `rook_types::NormalId` (the face frames come in
// `g.faces`, filled from it): a point belongs to its vertices' box face (the
// round primitives' grid patches, `mesh.rs`), or else to the face its normal
// is closest to, and its place on that face's image is measured in studs
// from the image's top-left corner, from its vertices' points on the box.

import package::common::{
    g, surface_atlas, image_sampler, VertexIn, VertexOut, ShadowIn,
    instance_rotation, world_position, placed, adorn_bits, CAST, NEON, UNLIT, EXACT, MODE_FACE, MODE_TILED,
    MODE_MESH_UV, UPRIGHT, SURFACES_SHIFT, MAPS, GLASS, SLOPED, MAPS_UV, VERTEX_COLORS,
    UNION_FACE, TERRAIN_FACE, TERRAIN_WATER, TERRAIN_DRAWN_SHIFT, CENTERED, ALPHA_OVERLAY, ALPHA_TRANSPARENCY,
    ALPHA_TINT_MASK, TERRAIN_RECOLOR, TERRAIN_TINT, TERRAIN_BLADES, PATTERN_SHIFT, one_tap, tile_taps, terrain_taps,
};
import package::images::{
    sample_image, image_size, sample_map, sample_taps, normal_taps, IMAGE, COLOR_MAP, ROUGHNESS_MAP,
    METALNESS_MAP, DETAIL_NORMAL_MAP,
};
import package::lighting::{shade, shade_pbr, shade_glass, glass_vertex, fog_units};
import package::tone::{albedo_units, frame, frame_of, linear_to_srgb, srgb_to_linear};
import package::terrain::{
    terrain_motion, terrain_position, terrain_share, water_surface, recolor, blade_surface,
    BLADE_FOOT_SUN, BLADE_TIP_SUN, blade_drawn,
};

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
    if packed >= TERRAIN_FACE && packed != 255u && (packed & TERRAIN_BLADES) != 0u
        && !blade_drawn(v.i_position, v.uv.x) {
        out.clip = vec4(0.0, 0.0, 0.0, 1.0);
    }
    out.world = world;
    out.view = world - g.camera_pos.xyz;
    out.normal = normalize(instance_rotation(v) * local_normal);
    out.color = vec4(srgb_to_linear(v.i_color.rgb), v.i_color.a);
    let is_terrain = packed >= TERRAIN_FACE && packed != 255u;
    let terrain_mode = (v.i_flags.x >> SURFACES_SHIFT) & 3u;
    if is_terrain && (v.i_flags.x & MAPS) != 0u
        && (terrain_mode == TERRAIN_RECOLOR || terrain_mode == TERRAIN_TINT) {
        // Not a linear color: how the color map is recolored (`recolor`),
        // or the sRGB tint it's multiplied by.
        out.color = v.i_color;
    }
    if (v.i_flags.x & VERTEX_COLORS) != 0u {
        let c = vec3((v.face >> 8u) & 255u, (v.face >> 16u) & 255u, v.face >> 24u);
        out.color = vec4(srgb_to_linear(vec3<f32>(c) / 255.0), v.i_color.a);
    }
    out.uv = v.uv;
    if is_terrain && (packed & TERRAIN_BLADES) != 0u {
        // A blade's foot (its vertices at height 0) and tip take their
        // shares of the sun, blended along it (`blade_surface`).
        out.uv.y = select(BLADE_TIP_SUN, BLADE_FOOT_SUN, v.uv.y <= 0.0);
    }
    let lo = v.i_box_min * v.i_size;
    let hi = v.i_box_max * v.i_size;
    var local = v.box_point * v.i_size;
    var normal = local_normal;
    // Union vertices (bit 6 of their face) tile from their face's center.
    let centered = packed != 255u && (packed & UNION_FACE) != 0u;
    var face = select(packed, packed & 7u, centered);
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
    out.face = face | select(0u, CENTERED, centered);
    if packed >= TERRAIN_FACE && packed != 255u {
        out.face |= v.face & (7u << TERRAIN_DRAWN_SHIFT);
    }
    out.local = local;
    out.local_normal = normal;
    out.box_center = center;
    out.box_size = size;
    out.tile = v.i_tile;
    out.flags = v.i_flags.x;
    out.material = v.i_material;
    out.images = v.i_flags.yzw;
    if (out.flags & GLASS) != 0u {
        out.glass = glass_vertex(world, out.normal, v.i_color.a);
    }
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
    var world = placed(v.position, v.i_position, rotation, v.i_size);
    // Waves and blades cast where they're drawn, culled blades not at all.
    let packed = v.face & 255u;
    if packed >= TERRAIN_FACE && packed != 255u {
        if (packed & TERRAIN_BLADES) != 0u && !blade_drawn(v.i_position, v.uv.x) {
            return vec4(0.0, 0.0, 0.0, 1.0);
        }
        world = terrain_motion(world, packed, v.uv);
    }
    return g.shadow_view_proj * vec4(world, 1.0);
}

// A highlighted part into the Highlight mask: its Highlight's number,
// carried in its color (`Instance::numbered`), where it covers. The
// position is worked out as `vs_main` does, so it meets the depth it left.
struct HighlightOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) @interpolate(flat) number: u32,
};

@vertex
fn vs_highlight(v: VertexIn) -> HighlightOut {
    var out: HighlightOut;
    out.clip = g.view_proj * vec4(world_position(v), 1.0);
    out.number = u32(round(v.i_color.r));
    return out;
}

@fragment
fn fs_highlight(in: HighlightOut) -> @location(0) u32 {
    return in.number;
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
// union its center (Studio tiles Textures on unions through the union's
// UVs, planar and in studs from its center; measured on a resized union,
// where the tiles sat a quarter stud from where corner tiling puts them).
// `at` is in studs from the top-left corner of a face of size `extent`.
fn tiling_position(in: VertexOut, at: vec2<f32>, extent: vec2<f32>) -> vec2<f32> {
    return select(at, at - extent * 0.5, (in.face & CENTERED) != 0u);
}

// Studs from the top-left corner of `face`'s image, and the face's size.
fn face_position(face: u32, local: vec3<f32>, center: vec3<f32>, size: vec3<f32>) -> vec4<f32> {
    let right = g.faces[face * 2u].xyz;
    let up = g.faces[face * 2u + 1u].xyz;
    let extent = vec2(dot(abs(right), size), dot(abs(up), size));
    let p = local - center;
    return vec4(dot(p, right) + extent.x * 0.5, extent.y * 0.5 - dot(p, up), extent);
}

// The slopes of a built-in material's detail normal map, added to its
// normal map's as Studio's part shaders do. Its repeats follow the plain
// coordinates `uv`, never the taps that break up the other maps' repeats.
fn detail_slopes(in: VertexOut, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec2<f32> {
    let strength = fract(in.material.w) * 2.0;
    if strength == 0.0 {
        return vec2(0.0);
    }
    let repeats = floor(in.material.w);
    let d = sample_map(in.images, DETAIL_NORMAL_MAP, uv * repeats, dx * repeats, dy * repeats);
    return (d.xy * 2.0 - 1.0) * strength;
}

// Normal maps fade out with depth into the view, gone from
// `NORMAL_FADE_STUDS`: Studio's part shaders scale the slopes, measured
// at docs/behavior/rendering/materials.md#normal-maps. `view` is the
// point from the camera; the view projection's last row reads its depth.
const NORMAL_FADE_STUDS: f32 = 1350.0;

fn faded_normal(t: vec3<f32>, view: vec3<f32>) -> vec3<f32> {
    let m = g.view_proj;
    let depth = dot(view, vec3(m[0].w, m[1].w, m[2].w));
    return vec3(t.xy * clamp(1.0 - depth / NORMAL_FADE_STUDS, 0.0, 1.0), t.z);
}

// A normal map's normal `t` (X right and Y up the image, Z out) turned
// into the world from the surface's normal `n`, using how the world
// position `p` and the map's coordinates change across the pixel (a
// cotangent frame, so no tangents need storing). The map's coordinates
// grow down the image.
//
// The frame's axes are the coordinates' gradients scaled by the pixel's
// area on the surface, whose sign says which way the screen's axes turn
// around `n`: negative under WGSL's `dpdy`, which runs down the screen, so
// without it every map drew inverted, bricks sunk where Studio raises
// them (measured, docs/behavior/rendering/materials.md#normal-maps).
fn perturb(n: vec3<f32>, t: vec3<f32>, dp_x: vec3<f32>, dp_y: vec3<f32>, duv_x: vec2<f32>, duv_y: vec2<f32>) -> vec3<f32> {
    let n0 = normalize(n);
    let y_perp = cross(dp_y, n0);
    let x_perp = cross(n0, dp_x);
    let tangent = y_perp * duv_x.x + x_perp * duv_y.x;
    let bitangent = y_perp * duv_x.y + x_perp * duv_y.y;
    let turn = select(1.0, -1.0, dot(cross(dp_x, dp_y), n0) < 0.0);
    let scale = turn * inverseSqrt(max(max(dot(tangent, tangent), dot(bitangent, bitangent)), 1e-20));
    return normalize(tangent * scale * t.x - bitangent * scale * t.y + n0 * t.z);
}

// Where a point is on its material's textures, in studs from the
// top-left corner of its face: as images go (`at`), except on wedges,
// whose faces use the face their own normal is closest to and lengths
// measured in their plane, so a long, low wedge's slope tiles like a top
// instead of stretching the Front face it belongs to for images over its
// whole length (reported on a large modern place). Along a slope Studio
// counts 0.7 x the two sides it spans, not its true length (Confirmed:
// its part vertex shaders scale a slope's texture coordinate by 0.7 x
// (Y + Z) size on wedges, 0.7 x (X + Y) on a corner wedge's other slope).
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
    let along = vec2(dot(p, r) + extent.x * 0.5, extent.y * 0.5 - dot(p, u));
    return along * vec2(slope_scale(r, in.box_size, extent.x), slope_scale(u, in.box_size, extent.y));
}

// Studio's length over the true one along `t`, a direction in a face
// `length` studs long that way: 0.7 x the box sides it spans when it
// crosses two axes (a slope), else 1.
fn slope_scale(t: vec3<f32>, size: vec3<f32>, length: f32) -> f32 {
    let spans = step(vec3(1e-3), abs(t));
    if dot(spans, vec3(1.0)) < 2.0 {
        return 1.0;
    }
    return 0.7 * dot(spans, size) / max(length, 1e-3);
}

// Neon's glow in light units from its linear color: 13.6 times the fourth
// power of its sRGB color, as bright as the frame keeps from about 188
// gray up (fitted in Studio 0.742: gray 128 is 0.865 units under both
// curves, 64 is 0.055, blue 105 0.38, and 160 gray reaches 2.04, as
// Decals over it show; docs/behavior/lighting/tone-mapping.md).
fn neon(albedo: vec3<f32>) -> vec3<f32> {
    let c = linear_to_srgb(albedo);
    return 13.6 * c * c * c * c;
}

// The flags past `SURFACES_SHIFT`: surface patterns, or what the maps mean.
// Not a varying of its own, as `VertexOut` is at the limit of 16 with
// `front_facing`.
fn surface_bits(in: VertexOut) -> u32 {
    return in.flags >> SURFACES_SHIFT;
}

// Parts and their mesh textures. A texture's alpha is ignored on an opaque
// part, but on one with any Transparency it multiplies the opacity,
// blended rather than cut (observed on a MeshPart's leaf texture at
// Transparency 0, 0.02, 0.1 and 0.5), and on adornments drawn without
// light (an ImageHandleAdornment's image; Inference).
@fragment
fn fs_main(in: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let image = sample_image(in.images, in.uv);
    var albedo = in.color.rgb * image.rgb;
    let face = face_at(in);
    let placed = face_position(face, in.local, in.box_center, in.box_size);
    let at = placed.xy;
    let tile = g.faces[0].w;
    let st = at / tile;
    let dx = dpdx(st);
    let dy = dpdy(st);
    // The material's textures tile like a Texture, from each face's
    // top-left corner (measured: unrotated, unmirrored), shifted by the
    // part's own start (`MaterialMaps::seed`, in `tile.zw`).
    let on_uv = (in.flags & MAPS_UV) != 0u;
    let tiled = tiling_position(in, material_position(in, face, at), placed.zw) + in.tile.zw;
    var uv = select(tiled / max(in.tile.xy, vec2(1e-3)), in.uv, on_uv);
    let packed = in.face & 255u;
    let terrain = packed >= TERRAIN_FACE && packed != 255u;
    let dp_x = dpdx(in.view);
    let dp_y = dpdy(in.view);
    var planar = vec2(0.0);
    var duv_x = dpdx(uv);
    var duv_y = dpdy(uv);
    if terrain {
        let cube = (packed & 1u) != 0u;
        let tile = max(in.tile.xy, vec2(1e-3));
        planar = terrain_position(in.world, in.normal, cube);
        uv = planar / tile;
        // The plane is the same across the pixel's neighbors, so its
        // footprint follows from the steps in position.
        duv_x = terrain_position(dp_x, in.normal, cube) / tile;
        duv_y = terrain_position(dp_y, in.normal, cube) / tile;
    }
    if terrain && (packed & TERRAIN_WATER) != 0u {
        return water_surface(in, dp_x, dp_y);
    }
    if terrain && (packed & TERRAIN_BLADES) != 0u {
        return blade_surface(in, dp_x, dp_y);
    }
    // Where the textures are read, their repeats broken up as Studio
    // breaks them (`tile_taps`, `terrain_taps`).
    var taps = one_tap(uv);
    if terrain {
        taps = terrain_taps(planar, max(in.tile.x, 1e-3), in.tile.z, in.tile.w);
    } else if !on_uv {
        taps = tile_taps(uv, (in.flags >> PATTERN_SHIFT) & 3u);
    }
    // Only a DoubleSided mesh shows its back faces, lit as if they faced
    // the viewer (`gl_FrontFacing` in `shaders_glsl3.pack`).
    let facing = select(-in.normal, in.normal, front);
    var normal = facing;
    var roughness = in.material.y;
    var metalness = in.material.z;
    var alpha = in.color.a;
    if in.color.a < 1.0 {
        alpha *= image.a;
    }
    if on_uv {
        let c = sample_map(in.images, COLOR_MAP, uv, duv_x, duv_y);
        let a = surface_appearance(in, c);
        albedo = a.rgb;
        alpha *= a.a;
    } else if terrain && (in.flags & MAPS) != 0u && (surface_bits(in) & 3u) == TERRAIN_RECOLOR {
        // Terrain's color maps are recolored by the material's color in
        // sRGB, before Studio's shader squares them (`recolor`).
        let c = sample_taps(in.images, COLOR_MAP, taps, duv_x, duv_y);
        albedo = srgb_to_linear(recolor(linear_to_srgb(c.rgb), in.color.rgb));
    } else if terrain && (in.flags & MAPS) != 0u && (surface_bits(in) & 3u) == TERRAIN_TINT {
        // The 2022 set's: tinted by the material's color where the map's
        // alpha says, in sRGB, as Studio's terrain shader does
        // (`rook_terrain::Coloring::Tint`).
        let c = sample_taps(in.images, COLOR_MAP, taps, duv_x, duv_y);
        albedo = srgb_to_linear(mix(vec3(1.0), in.color.rgb, c.a) * linear_to_srgb(c.rgb));
    } else if (in.flags & MAPS) != 0u && in.material.w > 0.0 {
        // The built-in materials': tinted by the part's color where the
        // map's alpha says, in sRGB (Studio's part shaders: mix(1, color,
        // alpha) x map), so a brick's mortar keeps its own pale color.
        let c = sample_taps(in.images, COLOR_MAP, taps, duv_x, duv_y);
        albedo = image.rgb * srgb_to_linear(mix(vec3(1.0), linear_to_srgb(in.color.rgb), c.a) * linear_to_srgb(c.rgb));
    } else if (in.flags & MAPS) != 0u {
        albedo *= sample_taps(in.images, COLOR_MAP, taps, duv_x, duv_y).rgb;
    }
    if (in.flags & MAPS) != 0u {
        roughness *= sample_taps(in.images, ROUGHNESS_MAP, taps, duv_x, duv_y).g;
        metalness *= sample_taps(in.images, METALNESS_MAP, taps, duv_x, duv_y).r;
        var t = normal_taps(in.images, taps, duv_x, duv_y);
        if !terrain && !on_uv {
            t = vec3(t.xy + detail_slopes(in, uv, duv_x, duv_y), t.z);
        }
        if !terrain {
            t = faded_normal(t, in.view);
        }
        if (in.flags & GLASS) == 0u {
            normal = perturb(facing, t, dp_x, dp_y, duv_x, duv_y);
        }
    }
    if surface_bits(in) != 0u && (in.flags & MAPS_UV) == 0u && !terrain {
        let pattern = (surface_bits(in) >> (face * 3u)) & 7u;
        if pattern != 0u {
            // The patterns are gray around 0.5: twice them leaves the
            // color as it is between the studs.
            let s = textureSampleGrad(surface_atlas, image_sampler, st, i32(pattern - 1u), dx, dy);
            albedo *= s.rgb * 2.0;
        }
    }
    if (adorn_bits(in.flags) & EXACT) != 0u {
        return vec4(frame_of(albedo), in.color.a * image.a);
    }
    if (adorn_bits(in.flags) & UNLIT) != 0u {
        return vec4(frame(fog_units(albedo_units(albedo), in.world)), in.color.a * image.a);
    }
    if (in.flags & NEON) != 0u {
        return vec4(frame(fog_units(neon(albedo), in.world)), in.color.a);
    }
    if (in.flags & GLASS) != 0u {
        let lit = shade_glass(
            albedo_units(albedo), in.world, normal, roughness, in.material.x, in.color.a, in.glass,
        );
        return vec4(frame(lit.rgb), lit.a);
    }
    let lit = shade_pbr(albedo_units(albedo), in.world, normal, roughness, metalness, in.material.x);
    var out = frame(lit);
    if terrain && (in.face >> TERRAIN_DRAWN_SHIFT) != 0u {
        out *= terrain_share(in);
    }
    return vec4(out, alpha);
}

// A SurfaceAppearance's color (linear) and opacity from its color map
// `c` (creator docs, checked in Studio 0.742 with a map whose see-through
// pixels are white): the map times its tint (`Color`), where Overlay shows
// the part's Color through the map's see-through pixels, Transparency
// makes them see-through, TintMask tints only where the map is opaque,
// and Opaque ignores the alpha. The part's Color counts only for Overlay.
fn surface_appearance(in: VertexOut, c: vec4<f32>) -> vec4<f32> {
    let tint = srgb_to_linear(vec3(in.tile.z, in.tile.w, in.material.w));
    let mode = surface_bits(in) & 3u;
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

// See-through Glass: Studio's Glass shaders multiply the light behind by
// mix(1, Color, 2 x (1 - Transparency)), Color as sRGB, and its tint pass
// does the same again, so the frame, which holds the light's square root,
// takes that once (read from the shaders; red, gray and black Glass over a
// wall of grays agree). Before the tone curve, as there.
@fragment
fn fs_glass_filter(in: VertexOut) -> @location(0) vec4<f32> {
    let tint = clamp(2.0 * in.color.a, 0.0, 1.0);
    return vec4(mix(vec3(1.0), linear_to_srgb(in.color.rgb), tint), 1.0);
}

// Depth alone (terrain water's first pass): the blend keeps the color.
@fragment
fn fs_depth() -> @location(0) vec4<f32> {
    return vec4(0.0);
}

// ForceField: its color added over what's behind, faint where it faces
// the camera and strong towards its outline, less as Transparency rises
// (observed on blocks and balls: face-on a red one added about 0.014 to
// the red behind it, at grazing angles 0.18; Approximation). Studio's
// moving pattern isn't drawn.
@fragment
fn fs_force_field(in: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let v = normalize(g.camera_pos.xyz - in.world);
    let rim = pow(1.0 - abs(dot(n, v)), 3.0);
    return vec4(frame_of(in.color.rgb * (0.025 + 0.6 * rim) * in.color.a), 0.0);
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
    return vec4(frame(shade(albedo_units(texel.rgb * in.color.rgb), in.world, in.normal)), alpha);
}
