// GUI rectangles (`renderer/gui.rs`): each instance a turned rectangle in
// pixels, filled, drawn through a glyph's coverage in the atlas, its image
// or a group's texture, cut to its shape and colored by its gradient,
// premultiplied into the GUI layer, which the tone pass lays over the
// finished frame in sRGB (`tonemap.wgsl`). A group's quads are drawn the
// same way into its texture first.

import package::gui_paint::{gui_gradient, gui_paint, gui_shape, ramp_uv};

struct Screen {
    // xy: the target's size in pixels.
    size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> screen: Screen;
@group(0) @binding(1) var glyphs: texture_2d<f32>;
@group(0) @binding(2) var glyph_sampler: sampler;
// Gradients' ramps, one a row (`GuiRamp`), raw sRGB and opacity.
@group(0) @binding(3) var ramps: texture_2d<f32>;
// The run's image (white for fills and glyphs), raw sRGB.
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct Quad {
    @location(0) center: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    // sin and cos of the clockwise turn.
    @location(2) turn: vec2<f32>,
    // sRGB, premultiplied by its opacity.
    @location(3) color: vec4<f32>,
    // left, top, right, bottom in pixels.
    @location(4) clip: vec4<f32>,
    // The glyph's left, top, right, bottom in the atlas, or the image's.
    @location(5) uv: vec4<f32>,
    // How it's drawn: `gui_paint`'s mode.
    @location(6) mode: f32,
    // Its shape's center and half size (0: none), in pixels.
    @location(7) shape_rect: vec4<f32>,
    // Its shape's corner radii (`gui_shape`).
    @location(8) radii: vec4<f32>,
    // Its shape's band and cut corners (`gui_shape`), and the gradient's
    // ramp row (below 0: none).
    @location(9) shape: vec4<f32>,
    // Where the gradient is 0.5, and its change per pixel.
    @location(10) gradient: vec4<f32>,
};

struct Varyings {
    @builtin(position) at: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) clip: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) mode: f32,
    // Where in its shape, from the center, unturned.
    @location(4) shape_at: vec2<f32>,
    @location(5) @interpolate(flat) shape_half: vec2<f32>,
    @location(6) @interpolate(flat) radii: vec4<f32>,
    @location(7) @interpolate(flat) shape: vec4<f32>,
    // Where along its gradient.
    @location(8) t: f32,
};

@vertex
fn vs_gui(@builtin(vertex_index) i: u32, q: Quad) -> Varyings {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
        vec2(-1.0, 1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
    );
    let c = corners[i] * q.half_size;
    let p = q.center + vec2(c.x * q.turn.y - c.y * q.turn.x, c.x * q.turn.x + c.y * q.turn.y);
    let ndc = vec2(p.x / screen.size.x * 2.0 - 1.0, 1.0 - p.y / screen.size.y * 2.0);
    var out: Varyings;
    out.at = vec4(ndc, 0.0, 1.0);
    out.color = q.color;
    out.clip = q.clip;
    let t = corners[i] * 0.5 + 0.5;
    out.uv = mix(q.uv.xy, q.uv.zw, t);
    out.mode = q.mode;
    let d = p - q.shape_rect.xy;
    out.shape_at = vec2(d.x * q.turn.y + d.y * q.turn.x, d.y * q.turn.y - d.x * q.turn.x);
    out.shape_half = q.shape_rect.zw;
    out.shape = q.shape;
    out.radii = q.radii;
    out.t = 0.5 + dot(p - q.gradient.xy, q.gradient.zw);
    return out;
}

@fragment
fn fs_gui(v: Varyings) -> @location(0) vec4<f32> {
    // Sampled before the clip test, which makes control flow non-uniform.
    let glyph = textureSample(glyphs, glyph_sampler, v.uv).r;
    let paint = gui_paint(glyph, textureSample(image, image_sampler, v.uv), v.mode);
    let cover = gui_shape(v.shape_at, v.shape_half, v.radii, v.shape);
    let rows = vec2<f32>(textureDimensions(ramps));
    let ramp = textureSample(ramps, glyph_sampler, ramp_uv(v.t, v.shape.z, rows.y, rows.x));
    let p = v.at.xy;
    if p.x < v.clip.x || p.y < v.clip.y || p.x > v.clip.z || p.y > v.clip.w {
        discard;
    }
    return v.color * paint * gui_gradient(ramp, v.shape.z) * cover;
}
