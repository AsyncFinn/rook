//! How Studio colors a material's textures by its `MaterialColors` color:
//! for the 2022 set, a texel's luma times a tint worked out per material
//! from the color, a brightness and a turned and scaled chroma. The
//! constants are fitted to Studio screenshots (`docs/behavior/terrain/materials.md`).

use rook_types::{Color3uint8, MaterialSet};

use super::TerrainMaterial;

/// One material's tint: `chroma` times the color's chroma, at most `cap`
/// long, and `gain` times its brightness, at most 1, times `scale`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tint {
    chroma: [[f32; 2]; 2],
    cap: f32,
    gain: f32,
    scale: f32,
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "builds the rows of a table of Option<Tint>, beside its None rows"
)]
const fn tint(chroma: [[f32; 2]; 2], cap: f32, gain: f32, scale: f32) -> Option<Tint> {
    Some(Tint {
        chroma,
        cap,
        gain,
        scale,
    })
}

const NONE: f32 = f32::INFINITY;

// Fitted numbers; Grass's 0.318 isn't 1/π.
#[allow(clippy::approx_constant)]
#[rustfmt::skip]
const TINTS: [Option<Tint>; 23] = [
    None, // Air
    None, // Water
    tint([[1.052, 0.379], [-0.318, 1.181]], NONE, 1.0, 1.0), // Grass
    tint([[0.100, 0.163], [-0.134, 0.121]], 0.095, 1.0, 1.0), // Slate
    tint([[0.168, -0.037], [0.033, 0.193]], NONE, 1.0, 1.0), // Concrete
    tint([[0.911, -0.035], [0.024, 1.052]], NONE, 1.0, 1.0), // Brick
    tint([[1.123, -0.042], [0.038, 1.313]], 0.432, 1.0, 1.0), // Sand
    tint([[1.215, -0.148], [0.133, 1.390]], 0.602, 1.0, 1.0), // WoodPlanks
    tint([[1.043, 0.007], [-0.016, 1.209]], 0.121, 1.16, 1.0), // Rock
    tint([[0.423, -0.095], [0.074, 0.485]], NONE, 1.0, 1.0), // Glacier
    tint([[1.312, 0.070], [-0.071, 1.524]], 0.161, 1.0, 1.0), // Snow
    tint([[1.236, -0.109], [0.104, 1.425]], 0.681, 1.0, 1.0), // Sandstone
    tint([[0.739, -0.219], [0.175, 0.848]], 0.337, 2.2, 0.85), // Mud
    tint([[-0.588, -0.048], [0.024, -0.640]], 0.124, 3.43, 0.6), // Basalt
    tint([[0.941, -0.026], [0.028, 1.078]], 0.456, 1.26, 1.0), // Ground
    tint([[1.871, 0.675], [-0.590, 2.104]], 0.97, 1.0, 1.0), // CrackedLava
    tint([[0.802, 0.240], [-0.209, 0.893]], 0.123, 1.04, 1.0), // Asphalt
    tint([[1.186, 0.040], [-0.041, 1.375]], 0.438, 1.0, 1.0), // Cobblestone
    tint([[1.101, -0.020], [0.011, 1.256]], 0.526, 1.0, 1.0), // Ice
    tint([[1.188, 0.443], [-0.415, 1.371]], 0.634, 1.0, 1.0), // LeafyGrass
    tint([[0.109, -0.063], [0.056, 0.115]], 0.014, 1.0, 1.0), // Salt
    tint([[1.221, -0.277], [0.254, 1.414]], 0.402, 1.0, 1.0), // Limestone
    tint([[1.161, -0.359], [0.329, 1.335]], 0.139, 1.0, 1.0), // Pavement
];

impl TerrainMaterial {
    /// How the material's textures in `set` are colored for its
    /// `MaterialColors` color (sRGB, 0 to 1 and above). The 2022 set's
    /// color maps are tint masks: where opaque, the texel's luma is
    /// multiplied by this (a strong color lifts some channels over the
    /// luma). The legacy set's are colored and opaque (their packs say
    /// `alphamode` 1, Studio's `avgAlbedoPre2022` alpha 255): the texel is
    /// multiplied by the color over the set's default, so the default
    /// color draws the texture as it is (Inference). Air and Water give
    /// the color itself.
    pub fn tint(self, color: Color3uint8, set: MaterialSet) -> [f32; 3] {
        let c = [color.r, color.g, color.b].map(|v| f32::from(v) / 255.0);
        if set == MaterialSet::Legacy {
            let Some(base) = self
                .color(None, set)
                .filter(|_| TINTS[self as usize].is_some())
            else {
                return c;
            };
            let base = [base.r, base.g, base.b].map(|v| f32::from(v.max(1)) / 255.0);
            return [0, 1, 2].map(|i| c[i] / base[i]);
        }
        let Some(t) = TINTS[self as usize] else {
            return c;
        };
        let value = c[0].max(c[1]).max(c[2]);
        if value <= 0.0 {
            return [0.0; 3];
        }
        let co = (c[0] - c[2]) / 2.0 / value;
        let cg = (2.0 * c[1] - c[0] - c[2]) / 4.0 / value;
        let [[a, b], [d, e]] = t.chroma;
        let (mut co, mut cg) = (a * co + b * cg, d * co + e * cg);
        let length = co.hypot(cg);
        if length > t.cap {
            co *= t.cap / length;
            cg *= t.cap / length;
        }
        let brightness = (t.gain * value).min(1.0) * t.scale;
        [1.0 + co - cg, 1.0 + cg, 1.0 - co - cg].map(|x| x * brightness)
    }
}
