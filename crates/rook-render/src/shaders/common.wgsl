// Bindings and the instance layout shared by every pass.

struct Globals {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_dir: vec4<f32>,
    // The sun's light on a white face square to it, in light units
    // (`Lighting::sun_color`).
    sun_color: vec4<f32>,
    ambient: vec4<f32>,
    outdoor_ambient: vec4<f32>,
    // rgb fog colour (sRGB)
    fog_color: vec4<f32>,
    // x = fog start, y = fog end, z = exposure (a multiplier on linear
    // light, `Lighting::exposure`)
    fog_range: vec4<f32>,
    // World to the shadow map's clip space (reverse Z).
    shadow_view_proj: mat4x4<f32>,
    // x = a texel in uv, y = normal offset in studs, z = depth bias,
    // w = how far shadows reach (0: no shadows this frame)
    shadow: vec4<f32>,
    // Each face's image right (even) and up (odd) in part axes, in
    // `NormalId` order; w = studs per surface pattern tile.
    faces: array<vec4<f32>, 12>,
    // From clip space to directions from the camera (the skybox).
    sky_from_clip: mat4x4<f32>,
    // Each skybox image's direction, right and up (`SKYBOX_FACES`).
    sky_faces: array<vec4<f32>, 18>,
    // x = seconds, y = WaterWaveSize, z = WaterWaveSpeed, w = how far
    // grass is drawn.
    terrain: vec4<f32>,
    // GlobalWind (xyz); w = water's phase, WaterWaveSpeed x seconds summed.
    wind: vec4<f32>,
    // The sky overhead, at the horizon and below (sRGB): water reflects the
    // first two, and all three light surfaces. sky_top.w =
    // EnvironmentDiffuseScale, dimmed at night; sky_horizon.w =
    // EnvironmentSpecularScale.
    sky_top: vec4<f32>,
    sky_horizon: vec4<f32>,
    sky_bottom: vec4<f32>,
    // Each skybox image's slot in the image table, when there is one.
    sky_images: array<vec4<u32>, 2>,
    // Each terrain layer's blend, two per layer: the height's multiplier
    // and offset, the edge's softness, studs per repeat; then the height
    // map's slot and whether it's laid like a box (`TerrainLayer`).
    terrain_layers: array<vec4<f32>, 138>,
};

@group(0) @binding(0) var<uniform> g: Globals;
// The surface patterns (studs, lattice, inlet, universal), one per layer.
@group(1) @binding(2) var surface_atlas: texture_2d_array<f32>;
// Linear, mipmapped, repeating: images and the surface patterns.
@group(1) @binding(3) var image_sampler: sampler;
// Water's normal frames, one per layer (X in alpha, Y in green).
@group(1) @binding(4) var water_normals: texture_2d_array<f32>;
// The sky's reflection cube, in light units (`renderer/lighting/sky.rs`).
@group(1) @binding(5) var sky_cube: texture_cube<f32>;
// Studio's split-sum table of the sky's reflection (the install's
// `brdfLUT.dds`, or Rook's fit of it): A and B by remapped roughness
// across and the cosine to the camera down.
@group(1) @binding(6) var brdf_table: texture_2d<f32>;
// Instance flags.
const CAST: u32 = 1u;
const NEON: u32 = 2u;
// Bits 4-6 of a base part without a tile pattern (bits 2-3 clear; an
// image layer keeps its face there): drawn in its own colour without
// light, through the tone curve or, with EXACT, past it; and turned to
// face the camera about its position (`PartInstance::faces_camera`).
const UNLIT: u32 = 16u;
const EXACT: u32 = 32u;
const FACES_CAMERA: u32 = 64u;

// The adornment bits (UNLIT, EXACT, FACES_CAMERA) of instance flags.
fn adorn_bits(flags: u32) -> u32 {
    return select(0u, flags & 112u, (flags & 12u) == 0u);
}
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
// A terrain seam vertex's face: from this bit, which of its triangle's
// corners' layer its mesh draws (`mesh::TERRAIN_DRAWN_SHIFT`).
const TERRAIN_DRAWN_SHIFT: u32 = 9u;
// Set on `VertexOut::face` for union vertices: tile from the face's centre.
const CENTRED: u32 = 256u;
const ALPHA_OVERLAY: u32 = 1u;
const ALPHA_TRANSPARENCY: u32 = 2u;
const ALPHA_TINT_MASK: u32 = 3u;

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
    // x: CAST, NEON, the layer's mode (bits 2-3) and face (bits 4-6),
    // UPRIGHT (bit 7), and the surface patterns (SURFACES_SHIFT); yzw:
    // the images' slots in the image table.
    @location(14) i_flags: vec4<u32>,
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
    // The images' slots in the image table.
    @location(13) @interpolate(flat) images: vec3<u32>,
};

// What the shadow pass reads: positions only, and where the instance is.
struct ShadowIn {
    @location(0) position: vec3<f32>,
    @location(5) i_position: vec3<f32>,
    @location(6) i_right: vec3<f32>,
    @location(7) i_up: vec3<f32>,
    @location(8) i_back: vec3<f32>,
    @location(9) i_size: vec3<f32>,
    @location(14) i_flags: vec4<u32>,
};

fn instance_rotation(v: VertexIn) -> mat3x3<f32> {
    if (adorn_bits(v.i_flags.x) & FACES_CAMERA) != 0u {
        return facing_camera(v.i_position);
    }
    return mat3x3<f32>(v.i_right, v.i_up, v.i_back);
}

// Axes at `at` whose back points at the camera, kept upright.
fn facing_camera(at: vec3<f32>) -> mat3x3<f32> {
    let back = normalize(g.camera_pos.xyz - at);
    var right = cross(vec3(0.0, 1.0, 0.0), back);
    if dot(right, right) < 1e-6 {
        right = vec3(1.0, 0.0, 0.0);
    }
    right = normalize(right);
    return mat3x3<f32>(right, cross(back, right), back);
}

// Where a vertex of an instance is in the world (every pass).
fn world_position(v: VertexIn) -> vec3<f32> {
    return placed(v.position, v.i_position, instance_rotation(v), v.i_size);
}

fn placed(p: vec3<f32>, at: vec3<f32>, rotation: mat3x3<f32>, size: vec3<f32>) -> vec3<f32> {
    return at + rotation * (p * size);
}

// How a material's textures repeat (`rook_types::TilePattern`), in bits
// 2-3 of a part's flags.
const PATTERN_SHIFT: u32 = 2u;
const PATTERN_ORGANIC: u32 = 1u;
const PATTERN_WANG: u32 = 2u;

// Where to read a repeating texture, so its repeats don't line up: up to
// three points (`uv`) blended by `weight`, each with how its copy is
// turned (cos, sin), which turns the derivatives and normals read there
// too. `sample_taps` and `normal_taps` (images.wgsl) read them.
struct Taps {
    uv: array<vec2<f32>, 3>,
    turn: array<vec2<f32>, 3>,
    weight: vec3<f32>,
    count: u32,
}

fn one_tap(uv: vec2<f32>) -> Taps {
    var t: Taps;
    t.uv[0] = uv;
    t.turn[0] = vec2(1.0, 0.0);
    t.weight = vec3(1.0, 0.0, 0.0);
    t.count = 1u;
    return t;
}

// A well-mixed 32-bit number for a lattice point (`salt` keeps uses apart).
fn lattice_hash(p: vec2<i32>, salt: u32) -> u32 {
    var h = (bitcast<u32>(p.x) * 0x8da6b343u) ^ (bitcast<u32>(p.y) * 0xd8163841u) ^ (salt * 0xcb1ab31fu);
    h ^= h >> 16u;
    h *= 0x7feb352du;
    h ^= h >> 15u;
    h *= 0x846ca68bu;
    h ^= h >> 16u;
    return h;
}

// Smooth noise from 0 to 1: a random value per whole `p`, blended
// bilinearly between them (as Studio reads its 32 × 32 white-noise image).
fn value_noise(p: vec2<f32>) -> f32 {
    let c = vec2<i32>(floor(p));
    let f = p - floor(p);
    let v = vec4(
        f32(lattice_hash(c, 1u) >> 8u),
        f32(lattice_hash(c + vec2(1, 0), 1u) >> 8u),
        f32(lattice_hash(c + vec2(0, 1), 1u) >> 8u),
        f32(lattice_hash(c + vec2(1, 1), 1u) >> 8u),
    ) / 16777216.0;
    return mix(mix(v.x, v.y, f.x), mix(v.z, v.w, f.x), f.y);
}

// A part's material textures at `uv` (in repeats) by its pattern, from
// Studio's part shaders (`DefaultUnified{Tiled,Detiled,Wang}FS`):
// - Organic: a smooth noise with one random value per repeat, times 8,
//   picks *i* (its whole part) and blends (by its fraction) the texture
//   shifted by sin(3*i*, 7*i*) and by sin(3(*i* + 1), 7(*i* + 1)) repeats
//   (measured: a MaterialVariant's shifts land on exactly these, and the
//   noise's scale is a repeat at 8 and at 4 studs per tile).
// - Wang: the image is 4 × 4 tiles; each quarter-repeat cell shows the
//   tile whose edges match its neighbours'. Every cell edge is one of two
//   kinds, at random; a tile's column in the image is 0, 1, 2 or 3 for
//   left and right edges of kinds (0, 0), (0, 1), (1, 1), (1, 0), and its
//   row likewise for top and bottom (the layout of Studio's
//   `wangIndex.dds`, which Rook doesn't use: its cells come from a hash).
fn tile_taps(uv: vec2<f32>, pattern: u32) -> Taps {
    if pattern == PATTERN_ORGANIC {
        let n = value_noise(uv) * 8.0;
        let i = floor(n);
        var t: Taps;
        t.uv[0] = uv + sin(vec2(3.0, 7.0) * i);
        t.uv[1] = uv + sin(vec2(3.0, 7.0) * (i + 1.0));
        t.turn[0] = vec2(1.0, 0.0);
        t.turn[1] = vec2(1.0, 0.0);
        t.weight = vec3(1.0 - (n - i), n - i, 0.0);
        t.count = 2u;
        return t;
    }
    if pattern == PATTERN_WANG {
        let q = uv * 4.0;
        let cell = vec2<i32>(floor(q));
        // Vertical edges (salt 2) at a cell's left, horizontal (3) at its top.
        let w = lattice_hash(cell, 2u) & 1u;
        let e = lattice_hash(cell + vec2(1, 0), 2u) & 1u;
        let n = lattice_hash(cell, 3u) & 1u;
        let s = lattice_hash(cell + vec2(0, 1), 3u) & 1u;
        let tile = vec2(f32(2u * w + (w ^ e)), f32(2u * n + (n ^ s)));
        return one_tap((tile + (q - floor(q))) * 0.25);
    }
    return one_tap(uv);
}

// A terrain layer's textures at `p` (studs across the plane it's laid on,
// `terrain_position`), `studs` per repeat, broken up by its `shift` and
// `turn` (`Detile`; Studio's `SmoothClusterVS`): each mesh vertex has a
// random r from 0 to 255; there the coordinates turn by
// asin((r / 128 - 1) × turn) and move by (r, floor(2.6651442 r)) × shift
// repeats, and across a triangle the corners' copies blend by their
// weights. Rook's terrain vertices don't carry r, so the corners are
// those of a 4-stud grid (a voxel) over the plane, split along a diagonal,
// each with r from a hash (Approximation: Studio's vertices sit near
// those points, moved by the surface's shape).
fn terrain_taps(p: vec2<f32>, studs: f32, shift: f32, turn: f32) -> Taps {
    let uv = p / studs;
    if turn == 0.0 && fract(shift) == 0.0 {
        // Whole repeats: every copy is the same.
        return one_tap(uv);
    }
    let q = p * 0.25;
    let cell = vec2<i32>(floor(q));
    let f = q - floor(q);
    var corners: array<vec2<i32>, 3>;
    var t: Taps;
    if f.x + f.y < 1.0 {
        corners = array(cell, cell + vec2(1, 0), cell + vec2(0, 1));
        t.weight = vec3(1.0 - f.x - f.y, f.x, f.y);
    } else {
        corners = array(cell + vec2(1, 1), cell + vec2(0, 1), cell + vec2(1, 0));
        t.weight = vec3(f.x + f.y - 1.0, 1.0 - f.x, 1.0 - f.y);
    }
    for (var k = 0u; k < 3u; k++) {
        let r = f32(lattice_hash(corners[k], 4u) & 255u);
        let s = (r / 128.0 - 1.0) * turn;
        let c = sqrt(max(1.0 - s * s, 0.0));
        t.uv[k] = vec2(uv.x * c + uv.y * s, uv.y * c - uv.x * s) + vec2(r, floor(r * 2.6651442)) * shift;
        t.turn[k] = vec2(c, s);
    }
    t.count = 3u;
    return t;
}

// Derivatives `d` of the plain coordinates, turned as a tap's copy is.
fn turned(d: vec2<f32>, turn: vec2<f32>) -> vec2<f32> {
    return vec2(d.x * turn.x + d.y * turn.y, d.y * turn.x - d.x * turn.y);
}
