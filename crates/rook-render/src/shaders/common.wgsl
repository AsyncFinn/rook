// Bindings and the instance layout shared by every pass.

struct Globals {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_dir: vec4<f32>,
    ambient: vec4<f32>,
    outdoor_ambient: vec4<f32>,
    // rgb fog colour, w = brightness
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
    // GlobalWind (xyz); w = water's phase, WaterWaveSpeed x seconds summed.
    wind: vec4<f32>,
    // The sky overhead and at the horizon (sRGB), which water reflects.
    sky_top: vec4<f32>,
    sky_horizon: vec4<f32>,
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

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

fn instance_rotation(v: VertexIn) -> mat3x3<f32> {
    return mat3x3<f32>(v.i_right, v.i_up, v.i_back);
}

// Where a vertex of an instance is in the world (every pass).
fn world_position(v: VertexIn) -> vec3<f32> {
    return placed(v.position, v.i_position, instance_rotation(v), v.i_size);
}

fn placed(p: vec3<f32>, at: vec3<f32>, rotation: mat3x3<f32>, size: vec3<f32>) -> vec3<f32> {
    return at + rotation * (p * size);
}
