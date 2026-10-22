// How much of a GUI quad's color (`renderer/gui.rs`) a pixel shows, from
// its samples of the glyph atlas and of its image: all of a fill (mode 0),
// a mask's coverage (1), a distance field's (2), smoothed across its middle
// level over about a pixel and a third on screen, as Roblox's GUI shader
// draws text (its UI pixel shader, 0.743: smoothstep about 0.5 by 0.65 ×
// fwidth), so text stays sharp at any size; or the image's texel,
// premultiplied (3). Multiply the quad's premultiplied color by it. Call
// it in uniform control flow.
fn gui_paint(glyph: f32, texel: vec4<f32>, mode: f32) -> vec4<f32> {
    let w = max(0.65 * fwidth(glyph), 1e-4);
    let field = smoothstep(0.5 - w, 0.5 + w, glyph);
    if mode > 2.5 {
        return vec4(texel.rgb * texel.a, texel.a);
    }
    if mode < 0.5 {
        return vec4(1.0);
    }
    return vec4(select(field, glyph, mode < 1.5));
}
