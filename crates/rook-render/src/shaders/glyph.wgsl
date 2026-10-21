// How much of a GUI quad (`renderer/gui.rs`) a pixel covers, from its
// sample of the glyph atlas: all of a fill (mode 0), a mask's coverage
// (mode 1), or a distance field's (mode 2), smoothed across its middle level
// over about a pixel and a third on screen, as Roblox's GUI shader draws text
// (its UI pixel shader, 0.743: smoothstep about 0.5 by 0.65 × fwidth), so
// text stays sharp at any size. Call it in uniform control flow.
fn glyph_alpha(sample: f32, mode: f32) -> f32 {
    let w = max(0.65 * fwidth(sample), 1e-4);
    let field = smoothstep(0.5 - w, 0.5 + w, sample);
    if mode < 0.5 {
        return 1.0;
    }
    return select(field, sample, mode < 1.5);
}
