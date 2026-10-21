// World GUI (`renderer/gui/world.rs`): each SurfaceGui's and BillboardGui's
// quads, laid on its canvas's plane in the world, so edges and text stay
// sharp up close, its text drawn from distance fields. A quad is filled or
// drawn through a glyph, in sRGB premultiplied by its opacity, as on the
// screen (`gui.wgsl`).
// LightInfluence 0 gives its color squared times Brightness in light
// units, as unlit adornments do (measured: a red SurfaceGui frame and an
// unlit red adornment both draw 223 through the default curve); 1 lights
// it as a surface facing the camera's side of it, as particles are lit.

import package::common::{g, image_sampler};
import package::glyph::glyph_alpha;
import package::tone::frame;
import package::lighting::{diffuse_light, fog_amount, fog_light, sun_light};

struct Canvas {
    // Its top left corner, and the world step for one canvas pixel right
    // and down.
    origin: vec4<f32>,
    right: vec4<f32>,
    down: vec4<f32>,
    // LightInfluence, Brightness, and 1 when it's seen only from in front
    // (a SurfaceGui).
    look: vec4<f32>,
};

@group(2) @binding(0) var<uniform> canvas: Canvas;
@group(2) @binding(1) var glyphs: texture_2d<f32>;

struct Quad {
    @location(0) center: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    // sin and cos of the clockwise turn.
    @location(2) turn: vec2<f32>,
    // sRGB, premultiplied by its opacity.
    @location(3) color: vec4<f32>,
    // left, top, right, bottom in canvas pixels.
    @location(4) clip: vec4<f32>,
    // The glyph's left, top, right, bottom in the atlas.
    @location(5) uv: vec4<f32>,
    // How it's drawn: `glyph_alpha`'s mode.
    @location(6) mode: f32,
};

struct QuadOut {
    @builtin(position) at: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) clip: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(5) @interpolate(flat) mode: f32,
    // Where on the canvas, in its pixels.
    @location(3) canvas_at: vec2<f32>,
    @location(4) world: vec3<f32>,
};

@vertex
fn vs_world_gui(@builtin(vertex_index) i: u32, q: Quad) -> QuadOut {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
        vec2(-1.0, 1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
    );
    let c = corners[i] * q.half_size;
    let p = q.center + vec2(c.x * q.turn.y - c.y * q.turn.x, c.x * q.turn.x + c.y * q.turn.y);
    let world = canvas.origin.xyz + canvas.right.xyz * p.x + canvas.down.xyz * p.y;
    var out: QuadOut;
    out.at = g.view_proj * vec4(world, 1.0);
    out.color = q.color;
    out.clip = q.clip;
    let t = corners[i] * 0.5 + 0.5;
    out.uv = mix(q.uv.xy, q.uv.zw, t);
    out.mode = q.mode;
    out.canvas_at = p;
    out.world = world;
    return out;
}

@fragment
fn fs_world_gui(in: QuadOut) -> @location(0) vec4<f32> {
    // Sampled before the tests, which make control flow non-uniform.
    let shape = glyph_alpha(textureSample(glyphs, image_sampler, in.uv).r, in.mode);
    let p = in.canvas_at;
    let normal = normalize(cross(canvas.down.xyz, canvas.right.xyz));
    let toward = g.camera_pos.xyz - in.world;
    let alpha = in.color.a * shape;
    // Inference: a SurfaceGui can't be seen from behind its face.
    let behind = canvas.look.z > 0.0 && dot(normal, toward) <= 0.0;
    if alpha <= 0.0 || behind || p.x < in.clip.x || p.y < in.clip.y || p.x > in.clip.z
        || p.y > in.clip.w {
        discard;
    }
    let c = in.color.rgb / in.color.a;
    let albedo = c * c;
    var units = albedo * canvas.look.y;
    let f = fog_amount(in.world);
    let lit_share = canvas.look.x;
    if lit_share > 0.0 {
        let n = select(-normal, normal, dot(normal, toward) > 0.0);
        let lit = mix(diffuse_light(albedo, in.world, n, sun_light(in.world, n)), fog_light(), f);
        units = mix(units, lit, lit_share);
    }
    let opacity = alpha * mix(1.0 - f, 1.0, lit_share);
    return vec4(frame(units * g.fog_range.z) * opacity, opacity);
}
