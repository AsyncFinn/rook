// World GUI (`renderer/gui/world.rs`): each SurfaceGui's or BillboardGui's
// canvas, drawn by the GUI pass into the canvas atlas (sRGB, premultiplied
// by coverage), laid in the world as a rectangle. LightInfluence 0 gives
// its colour squared times Brightness in light units, as unlit
// adornments do (measured: a red SurfaceGui frame and an unlit red
// adornment both draw 223 through the default curve); 1 lights it as a
// surface facing the camera's side of it, as particles are lit.

import package::common::{g, image_sampler};
import package::tone::frame;
import package::lighting::{diffuse_light, fog_amount, fog_light, sun_light};

@group(2) @binding(0) var canvases: texture_2d<f32>;

struct CanvasIn {
    @builtin(vertex_index) corner: u32,
    // The rectangle's top left corner, and its whole width and height
    // as world vectors.
    @location(0) origin: vec3<f32>,
    @location(1) right: vec3<f32>,
    @location(2) down: vec3<f32>,
    // Its part of the atlas: left, top, right, bottom, 0 to 1.
    @location(3) uv: vec4<f32>,
    // LightInfluence and Brightness.
    @location(4) look: vec2<f32>,
    // 1 for a SurfaceGui, seen only from in front; 0 for a BillboardGui.
    @location(5) one_sided: f32,
};

struct CanvasOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world: vec3<f32>,
    @location(2) @interpolate(flat) normal: vec3<f32>,
    @location(3) @interpolate(flat) look: vec2<f32>,
    @location(4) @interpolate(flat) one_sided: f32,
};

@vertex
fn vs_world_gui(v: CanvasIn) -> CanvasOut {
    let corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    );
    let c = corners[v.corner];
    let world = v.origin + v.right * c.x + v.down * c.y;
    var out: CanvasOut;
    out.clip = g.view_proj * vec4(world, 1.0);
    out.uv = mix(v.uv.xy, v.uv.zw, c);
    out.world = world;
    out.normal = normalize(cross(v.down, v.right));
    out.look = v.look;
    out.one_sided = v.one_sided;
    return out;
}

@fragment
fn fs_world_gui(in: CanvasOut) -> @location(0) vec4<f32> {
    let texel = textureSample(canvases, image_sampler, in.uv);
    let toward = g.camera_pos.xyz - in.world;
    // Inference: a SurfaceGui can't be seen from behind its face.
    if texel.a <= 0.0 || (in.one_sided > 0.0 && dot(in.normal, toward) <= 0.0) {
        discard;
    }
    let c = texel.rgb / texel.a;
    let albedo = c * c;
    var units = albedo * in.look.y;
    let f = fog_amount(in.world);
    if in.look.x > 0.0 {
        let n = select(-in.normal, in.normal, dot(in.normal, toward) > 0.0);
        let lit = mix(diffuse_light(albedo, in.world, n, sun_light(in.world, n)), fog_light(), f);
        units = mix(units, lit, in.look.x);
    }
    let opacity = texel.a * mix(1.0 - f, 1.0, in.look.x);
    return vec4(frame(units * g.fog_range.z) * opacity, opacity);
}
