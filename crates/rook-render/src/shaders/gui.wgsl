// GUI rectangles (`renderer/gui.rs`): each instance a turned rectangle in
// pixels, drawn premultiplied into the GUI layer, which the tone pass lays
// over the finished frame in sRGB (`tonemap.wgsl`).

struct Screen {
    // xy: the target's size in pixels.
    size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> screen: Screen;

struct Quad {
    @location(0) center: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    // sin and cos of the clockwise turn.
    @location(2) turn: vec2<f32>,
    // sRGB, premultiplied by its opacity.
    @location(3) color: vec4<f32>,
    // left, top, right, bottom in pixels.
    @location(4) clip: vec4<f32>,
};

struct Varyings {
    @builtin(position) at: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) clip: vec4<f32>,
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
    return out;
}

@fragment
fn fs_gui(v: Varyings) -> @location(0) vec4<f32> {
    let p = v.at.xy;
    if p.x < v.clip.x || p.y < v.clip.y || p.x > v.clip.z || p.y > v.clip.w {
        discard;
    }
    return v.color;
}
