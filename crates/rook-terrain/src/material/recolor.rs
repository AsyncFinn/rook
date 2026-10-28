//! How Studio colors a material's textures by its `MaterialColors` color:
//! it moves each texel in YCoCg by how the color differs from the
//! material's default (`docs/behavior/terrain/materials.md#colors`).

use rook_types::Color3uint8;

use super::{TerrainMaterial, looks};

/// How far a color may brighten or saturate a texture: twice its default.
const MOST: f32 = 2.0;

/// What a material's color does to each texel of its textures (sRGB, 0 to
/// 1): its luma Y is multiplied by `gain`, and its chroma (Co, Cg) turned
/// and scaled by `turn` (the cosine and sine of the turn, times the
/// chroma's scale and `gain`). Y stays within 0 to 1 and Co and Cg within
/// ±0.5, as the YCoCg texture Studio builds stores them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recolor {
    pub gain: f32,
    pub turn: [f32; 2],
}

impl Recolor {
    /// Texels as they are: a material in its default color.
    pub const NONE: Self = Self {
        gain: 1.0,
        turn: [1.0, 0.0],
    };

    /// The texel `srgb` recolored (channels may pass 1 where the chroma is
    /// strong, as Studio's do).
    pub fn apply(self, [r, g, b]: [f32; 3]) -> [f32; 3] {
        let y = (r + 2.0 * g + b) / 4.0;
        let co = (r - b) / 2.0;
        let cg = (2.0 * g - r - b) / 4.0;
        let [c, s] = self.turn;
        let y = (y * self.gain).clamp(0.0, 1.0);
        let co2 = (c * co - s * cg).clamp(-0.5, 0.5);
        let cg2 = (s * co + c * cg).clamp(-0.5, 0.5);
        [y + co2 - cg2, y + cg2, y - co2 - cg2].map(|v| v.max(0.0))
    }

    /// As three numbers: gain, then the turn.
    pub fn to_array(self) -> [f32; 3] {
        [self.gain, self.turn[0], self.turn[1]]
    }
}

/// HSV hue (in turns), saturation and value.
fn hsv([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let range = max - r.min(g).min(b);
    let hue = if range <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / range).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / range + 2.0) / 6.0
    } else {
        ((r - g) / range + 4.0) / 6.0
    };
    let saturation = if max > 0.0 { range / max } else { 0.0 };
    [hue, saturation, max]
}

impl TerrainMaterial {
    /// How its textures are recolored for its `MaterialColors` color, in
    /// places of either material set (Studio draws terrain with the
    /// legacy set's textures in both). Against the material's legacy
    /// default color: the texel's chroma turns by the color's HSV hue less
    /// the default's, and scales by the color's saturation over the
    /// default's, at most twice; its luma, and its chroma again, scale by
    /// the color's value over the default's, at most twice. So the
    /// default color draws the texture as it is, white and gray draw it
    /// gray, and red turns Grass magenta and Basalt teal. Air and Water
    /// have no texture.
    pub fn recolor(self, color: Color3uint8) -> Recolor {
        let [r, g, b] = looks::BASE_LEGACY[self as usize];
        if [r, g, b] == [0; 3] {
            return Recolor::NONE;
        }
        let unit = |c: [u8; 3]| c.map(|v| f32::from(v) / 255.0);
        let [hc, sc, vc] = hsv(unit([color.r, color.g, color.b]));
        let [hd, sd, vd] = hsv(unit([r, g, b]));
        let gain = (vc / vd).min(MOST);
        // No material's default is a gray; were one, its own color would
        // still leave the texture as it is.
        let saturation = match (sd > 0.0, sc > 0.0) {
            (true, _) => (sc / sd).min(MOST),
            (false, true) => MOST,
            (false, false) => 1.0,
        };
        let angle = (hc - hd) * std::f32::consts::TAU;
        let scale = gain * saturation;
        Recolor {
            gain,
            turn: [angle.cos() * scale, angle.sin() * scale],
        }
    }
}
