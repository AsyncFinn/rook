// How much of a GUI quad's color (`renderer/gui.rs`) a pixel shows, from
// its samples of the glyph atlas and of its image: all of a fill (mode 0),
// a mask's coverage (1), a distance field's (2), smoothed across its middle
// level over about a pixel and a third on screen, as Roblox's GUI shader
// draws text (its UI pixel shader, 0.743: smoothstep about 0.5 by 0.65 ×
// fwidth), so text stays sharp at any size; the image's texel,
// premultiplied (3); or a group's texel, drawn premultiplied (4). Multiply
// the quad's premultiplied color by it. Call it in uniform control flow.
fn gui_paint(glyph: f32, texel: vec4<f32>, mode: f32) -> vec4<f32> {
    let w = max(0.65 * fwidth(glyph), 1e-4);
    let field = smoothstep(0.5 - w, 0.5 + w, glyph);
    if mode > 3.5 {
        return texel;
    }
    if mode > 2.5 {
        return vec4(texel.rgb * texel.a, texel.a);
    }
    if mode < 0.5 {
        return vec4(1.0);
    }
    return vec4(select(field, glyph, mode < 1.5));
}

// A rounded rectangle's signed distance at `p` from its center (y down):
// `half` its half size, `radii` its corners' (top left, top right, bottom
// right, bottom left), cut straight across when `bevel`.
fn shape_distance(p: vec2<f32>, half: vec2<f32>, radii: vec4<f32>, bevel: bool) -> f32 {
    let side = select(vec2(radii.x, radii.w), vec2(radii.y, radii.z), p.x > 0.0);
    let r = select(side.x, side.y, p.y > 0.0);
    let a = abs(p);
    if bevel {
        let q = a - half;
        let box_d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0);
        return max(box_d, (a.x + a.y - (half.x + half.y - r)) * 0.70710678);
    }
    let q = a - half + r;
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// How much of a pixel at `p` (from the shape's center, unturned, in the
// quad's pixels) a GUI shape covers (`GuiShape`): `half` its half size (0
// for none), `radii` its corner radii, `shape` its band and 1 for cut
// corners. The edge is smoothed over a pixel on screen, as Studio's
// UICorner is (measured: one blended pixel across a curved edge,
// docs/behavior/gui/modern.md#uicorner). Call it in uniform control flow.
fn gui_shape(p: vec2<f32>, half: vec2<f32>, radii: vec4<f32>, shape: vec4<f32>) -> f32 {
    let bevel = shape.y > 0.5;
    let outer = shape_distance(p, half, radii, bevel);
    let inner_half = half - shape.x;
    let inner = shape_distance(p, max(inner_half, vec2(0.0)), max(radii - shape.x, vec4(0.0)), bevel);
    let fo = max(length(vec2(dpdx(outer), dpdy(outer))), 1e-4);
    let fi = max(length(vec2(dpdx(inner), dpdy(inner))), 1e-4);
    let cover = clamp(0.5 - outer / fo, 0.0, 1.0);
    let hole = clamp(0.5 - inner / fi, 0.0, 1.0);
    let banded = shape.x > 0.0 && inner_half.x > 0.0 && inner_half.y > 0.0;
    if half.x <= 0.0 {
        return 1.0;
    }
    return select(cover, cover * (1.0 - hole), banded);
}

// Where along a gradient's ramp texture `t` (held at 0 and 1) samples, on
// row `row` of `rows`, the texels' centers spanning 0 to 1.
fn ramp_uv(t: f32, row: f32, rows: f32, texels: f32) -> vec2<f32> {
    let u = (clamp(t, 0.0, 1.0) * (texels - 1.0) + 0.5) / texels;
    return vec2(u, (max(row, 0.0) + 0.5) / rows);
}

// A gradient's texel (`GuiGradient`, sRGB and opacity) as a factor on a
// premultiplied color: 1 when the quad has none (`row` below 0).
fn gui_gradient(texel: vec4<f32>, row: f32) -> vec4<f32> {
    return select(vec4(texel.rgb * texel.a, texel.a), vec4(1.0), row < 0.0);
}
