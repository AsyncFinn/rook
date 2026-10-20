//! Mip chains: each level half the one before, down to 1 × 1.

/// The mip chain of an RGBA8 image of `width` × `height`, starting with
/// `level0` itself.
pub fn mips(level0: Vec<u8>, width: u32, height: u32) -> Vec<Vec<u8>> {
    let mut mips = vec![level0];
    let (mut w, mut h) = (width, height);
    while w > 1 || h > 1 {
        let next = downsample(mips.last().expect("level"), w, h);
        mips.push(next);
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    mips
}

/// Halve an RGBA8 image (a side of 1 stays 1), averaging each 2 × 2 block
/// weighted by alpha, so fully transparent pixels don't bleed their colour
/// into the edges of what's drawn.
pub(super) fn downsample(src: &[u8], w: u32, h: u32) -> Vec<u8> {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    let at = |x: u32, y: u32| {
        let i = ((y.min(h - 1) * w + x.min(w - 1)) * 4) as usize;
        &src[i..i + 4]
    };
    for y in 0..nh {
        for x in 0..nw {
            let (sx, sy) = (x * 2, y * 2);
            let block = [
                at(sx, sy),
                at(sx + 1, sy),
                at(sx, sy + 1),
                at(sx + 1, sy + 1),
            ];
            let alpha: u32 = block.iter().map(|p| u32::from(p[3])).sum();
            for c in 0..3 {
                let weighted: u32 = block
                    .iter()
                    .map(|p| u32::from(p[c]) * u32::from(p[3]))
                    .sum();
                // All transparent: a plain average.
                let v = (weighted + alpha / 2)
                    .checked_div(alpha)
                    .unwrap_or_else(|| block.iter().map(|p| u32::from(p[c])).sum::<u32>() / 4);
                out.push(v as u8);
            }
            out.push(((alpha + 2) / 4) as u8);
        }
    }
    out
}
