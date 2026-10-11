// Parts: one draw per shape, one instance per part.
//
// Lighting is an Approximation of Roblox's look until it is tuned against
// Studio screenshots: Lambert sun light plus Ambient and an OutdoorAmbient
// term that fades on faces pointing down, then linear fog.

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
};

@group(0) @binding(0) var<uniform> g: Globals;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) i_position: vec3<f32>,
    @location(3) i_right: vec3<f32>,
    @location(4) i_up: vec3<f32>,
    @location(5) i_back: vec3<f32>,
    @location(6) i_size: vec3<f32>,
    @location(7) i_color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    let rot = mat3x3<f32>(v.i_right, v.i_up, v.i_back);
    let world = v.i_position + rot * (v.position * v.i_size);
    // Inverse-transpose of a rotation times a scale: rotate n / size.
    let n = normalize(rot * (v.normal / max(v.i_size, vec3(1e-4))));
    var out: VertexOut;
    out.clip = g.view_proj * vec4(world, 1.0);
    out.world = world;
    out.normal = n;
    out.color = vec4(srgb_to_linear(v.i_color.rgb), v.i_color.a);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let sun = max(dot(n, g.sun_dir.xyz), 0.0) * g.fog_color.w * 0.5;
    let sky = 0.5 + 0.5 * n.y;
    let ambient = srgb_to_linear(g.ambient.rgb) + srgb_to_linear(g.outdoor_ambient.rgb) * sky;
    var color = in.color.rgb * (ambient + vec3(sun));
    let dist = distance(in.world, g.camera_pos.xyz);
    let fog = clamp((dist - g.fog_range.x) / max(g.fog_range.y - g.fog_range.x, 1e-3), 0.0, 1.0);
    color = mix(color, srgb_to_linear(g.fog_color.rgb), fog);
    return vec4(color, in.color.a);
}
