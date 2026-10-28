//! How Studio colors a material's textures by its `MaterialColors` color
//! (`docs/behavior/terrain/materials.md#colors`): the legacy set's by
//! moving each texel in YCoCg by how the color differs from the material's
//! default, the 2022 set's by tinting them by the color where their mask
//! says.

use rook_types::{Color3uint8, MaterialSet};

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

/// How a material's textures take its color in a place of one set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Coloring {
    /// The legacy set's: each texel moved ([`Recolor`]).
    Recolor(Recolor),
    /// The 2022 set's: each texel (sRGB) times `mix(1, color, alpha)`, the
    /// color map's alpha its tint mask, as Studio's terrain shader does
    /// for layers it gives a tint (`shaders_glsl3.pack`'s terrain shaders;
    /// measured: Mud's 2022 texture, its average albedo 193 of 255, draws
    /// 107 of 255 lit in its default color (121, 112, 98) beside Sand
    /// drawn as the same rule gives it).
    Tint([f32; 3]),
}

impl Coloring {
    /// The texel `srgb` (its alpha the tint mask) colored.
    pub fn apply(self, srgb: [f32; 3], mask: f32) -> [f32; 3] {
        match self {
            Coloring::Recolor(r) => r.apply(srgb),
            Coloring::Tint(t) => [0, 1, 2].map(|i| (1.0 + (t[i] - 1.0) * mask) * srgb[i]),
        }
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
    /// How its textures in a place of `set` take `color`.
    pub fn coloring(self, set: MaterialSet, color: Color3uint8) -> Coloring {
        match set {
            MaterialSet::Legacy => Coloring::Recolor(self.recolor(color)),
            MaterialSet::Materials2022 => {
                Coloring::Tint([color.r, color.g, color.b].map(|v| f32::from(v) / 255.0))
            }
        }
    }

    /// How its legacy textures are recolored for its `MaterialColors`
    /// color, against the material's legacy default color: the texel's chroma turns by the color's HSV hue less
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
