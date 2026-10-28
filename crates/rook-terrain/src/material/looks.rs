//! Each terrain material's shape and textures, from Studio's
//! `FStringTerrainMaterialTablePre2022` setting (Studio 0.742's
//! `ClientSettings`), in `TerrainMaterial` order. See [`Look`]. Studio
//! draws terrain with this table in places of either material set, so its
//! `FStringTerrainMaterialTable2022` isn't kept.

use super::{Blend, Edges, Look, Texture};

const fn tex(pack: u64, tiling: f32, detiling: f32, rotation: f32, blend: Blend) -> Texture {
    Texture {
        pack,
        tiling,
        detiling,
        rotation,
        blend,
    }
}

const fn blend(min: f32, max: f32, scale: f32) -> Blend {
    Blend { min, max, scale }
}

#[rustfmt::skip]
pub(super) const LOOKS: [Look; 23] = [
    // Air
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: None, side: None, bottom: None },
    // Water
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: None, side: None, bottom: None },
    // Grass
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547336797, 0.25, 4.0, 1.0, blend(0.0829, 1.0, 0.435))), side: Some(tex(7547623564, 0.2, 5.0, 0.0, blend(0.067, 0.933, 0.1998))), bottom: Some(tex(7551928998, 0.45, 2.2219, 0.0, blend(0.05, 1.3329, 0.5399))) },
    // Slate
    Look { shift: 0.1, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547627210, 0.55, 1.818, 0.0, blend(0.25, 1.0, 0.12))), side: Some(tex(7547627210, 0.55, 1.818, 0.0, blend(0.25, 1.0, 0.12))), bottom: Some(tex(7547627210, 0.55, 1.818, 0.0, blend(0.25, 1.0, 0.12))) },
    // Concrete
    Look { shift: 0.0, cubify: 0.0, quantize: 0.5, edges: Edges::Hard, cube: false, top: Some(tex(7547337977, 0.5, 1.25, 0.0, blend(-0.05, 1.0, 0.0552))), side: Some(tex(7547630026, 0.4, 2.5, 0.0, blend(-0.05, 0.9498, 0.1602))), bottom: Some(tex(7547630026, 0.4, 2.5, 0.0, blend(-0.05, 0.9498, 0.1602))) },
    // Brick
    Look { shift: 0.0, cubify: 1.0, quantize: 0.0, edges: Edges::Hard, cube: true, top: Some(tex(7547633775, 0.8, 5.0, 0.0, blend(0.1668, 0.767, 0.1397))), side: Some(tex(7547633775, 0.8, 5.0, 0.0, blend(0.1668, 0.767, 0.1397))), bottom: Some(tex(7547633775, 0.8, 5.0, 0.0, blend(0.1668, 0.767, 0.1397))) },
    // Sand
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547636223, 0.25, 4.0, 0.0, blend(0.017, 1.0499, 0.3552))), side: Some(tex(7546659291, 0.25, 1.818, 0.0, blend(0.1668, 0.9829, 0.3947))), bottom: Some(tex(7546659291, 0.25, 1.818, 0.0, blend(0.1668, 0.9829, 0.3947))) },
    // WoodPlanks
    Look { shift: 0.0, cubify: 1.0, quantize: 0.0, edges: Edges::Hard, cube: true, top: Some(tex(7547642685, 0.8, 2.0, 0.0, blend(0.15, 0.883, 9.9999e-05))), side: Some(tex(7547642685, 0.8, 2.0, 0.0, blend(0.15, 0.883, 9.9999e-05))), bottom: Some(tex(7547642685, 0.8, 2.0, 0.0, blend(0.15, 0.883, 9.9999e-05))) },
    // Rock
    Look { shift: 0.2998, cubify: 0.0, quantize: 0.0, edges: Edges::HardSoft, cube: false, top: Some(tex(7547645700, 0.55, 1.818, 0.5, blend(0.05, 0.4, 0.0798))), side: Some(tex(7547645700, 0.55, 1.818, 0.5, blend(0.05, 0.4, 0.0798))), bottom: Some(tex(7547645700, 0.55, 1.818, 0.5, blend(0.05, 0.4, 0.0798))) },
    // Glacier
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7551935562, 0.23, 1.818, 0.0, blend(0.2669, 0.967, 0.3552))), side: Some(tex(7551944502, 0.2, 1.1108, 0.0, blend(0.1168, 0.833, 0.165))), bottom: Some(tex(7551949364, 0.23, 1.6653, 0.0, blend(0.4499, 1.1, 0.3197))) },
    // Snow
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7551952124, 0.2998, 3.335_557, 0.0, blend(-0.067, 0.8999, 0.4649))), side: Some(tex(7551952124, 0.2998, 3.335_557, 0.0, blend(-0.067, 0.8999, 0.4649))), bottom: Some(tex(7551952124, 0.2998, 3.335_557, 0.0, blend(-0.067, 0.8999, 0.4649))) },
    // Sandstone
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7551956653, 0.5, 1.429592, 0.5, blend(0.067, 1.2, 0.2849))), side: Some(tex(7551965250, 0.2998, 2.0, 0.0, blend(0.05, 0.6169, 0.225))), bottom: Some(tex(7552020001, 0.45, 3.333, 0.0, blend(-0.0829, 1.1, 0.4199))) },
    // Mud
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7552025321, 0.2998, 10.0, 0.0, blend(0.1668, 0.933, 0.3149))), side: Some(tex(7552025321, 0.2998, 10.0, 0.0, blend(0.1668, 0.933, 0.3149))), bottom: Some(tex(7552025321, 0.2998, 10.0, 0.0, blend(0.1668, 0.933, 0.3149))) },
    // Basalt
    Look { shift: 0.2, cubify: 0.0, quantize: 0.0, edges: Edges::HardSoft, cube: false, top: Some(tex(7551978967, 0.55, 1.818, 0.0, blend(0.1, 1.0326, 0.0402))), side: Some(tex(7551978967, 0.55, 1.818, 0.0, blend(0.1, 1.0326, 0.0402))), bottom: Some(tex(7551978967, 0.55, 1.818, 0.0, blend(0.1, 1.0326, 0.0402))) },
    // Ground
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547349135, 0.2998, 3.333, 0.5, blend(0.2669, 0.933, 0.195))), side: Some(tex(7547349135, 0.2998, 3.333, 0.5, blend(0.2669, 0.933, 0.195))), bottom: Some(tex(7547349135, 0.2998, 3.333, 0.5, blend(0.2669, 0.933, 0.195))) },
    // CrackedLava
    Look { shift: 0.1, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7551983669, 0.28, 3.333, 0.5, blend(-0.067, 0.85, 0.2148))), side: Some(tex(7551983669, 0.28, 3.333, 0.5, blend(-0.067, 0.85, 0.2148))), bottom: Some(tex(7551983669, 0.28, 3.333, 0.5, blend(-0.067, 0.85, 0.2148))) },
    // Asphalt
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7551987784, 0.25, 4.3478, 0.0, blend(0.3, 1.0326, 0.24))), side: Some(tex(7551990829, 0.2, 1.1108, 0.0, blend(-0.017, 0.967, 0.1096))), bottom: Some(tex(7551990829, 0.2, 1.1108, 0.0, blend(-0.017, 0.967, 0.1096))) },
    // Cobblestone
    Look { shift: 0.0, cubify: 0.0, quantize: 0.1499, edges: Edges::HardSoft, cube: false, top: Some(tex(7551995071, 0.23, 4.0, 0.0, blend(0.133, 1.167, 0.2597))), side: Some(tex(7547650812, 0.2, 1.1108, 0.0, blend(-0.1, 0.5669, 0.1998))), bottom: Some(tex(7547650812, 0.2, 1.1108, 0.0, blend(-0.1, 0.5669, 0.1998))) },
    // Ice
    Look { shift: 0.1499, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547655101, 0.25, 4.0, 0.1, blend(0.0, 1.0, 0.1))), side: Some(tex(7547659308, 0.25, 4.0, 0.0, blend(0.0, 1.0, 0.1))), bottom: Some(tex(7547659308, 0.25, 4.0, 0.0, blend(0.0, 1.0, 0.1))) },
    // LeafyGrass
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7546664715, 0.25, 3.0299, 1.0, blend(0.017, 0.767, 0.2898))), side: Some(tex(7546666342, 0.25, 4.0, 0.0, blend(0.133, 0.75, 0.1902))), bottom: Some(tex(7546666342, 0.25, 4.0, 0.0, blend(0.133, 0.75, 0.1902))) },
    // Salt
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547663777, 0.23, 1.818, 0.0, blend(0.1668, 1.0, 0.3251))), side: Some(tex(7547669153, 0.2, 1.1108, 0.0, blend(-0.067, 0.9168, 0.2597))), bottom: Some(tex(7547669153, 0.2, 1.1108, 0.0, blend(-0.067, 0.9168, 0.2597))) },
    // Limestone
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547673056, 0.25, 1.818, 0.1, blend(0.1668, 0.9168, 0.3498))), side: Some(tex(7547676869, 0.25, 2.0, 0.0, blend(0.0, 0.967, 0.135))), bottom: Some(tex(7547676869, 0.25, 2.0, 0.0, blend(0.0, 0.967, 0.135))) },
    // Pavement
    Look { shift: 0.0, cubify: 0.0, quantize: 0.25, edges: Edges::HardSoft, cube: false, top: Some(tex(7547679257, 0.46, 4.0, 0.0, blend(0.2169, 0.9829, 0.12))), side: Some(tex(7547684004, 0.46, 4.0, 0.0, blend(-0.133, 1.067, 0.1152))), bottom: Some(tex(7547684004, 0.46, 4.0, 0.0, blend(-0.133, 1.067, 0.1152))) },
];

/// Each material's default color per set, from Studio's
/// `FStringTerrainColorTable` setting (`baseColorsPre2022` and
/// `baseColors2022`), in `TerrainMaterial` order. The legacy set's are
/// the colors [`LOOKS`]' textures draw as they are
/// ([`super::Recolor`]); a new place saves the 2022 set's.
#[rustfmt::skip]
pub(super) const BASE_LEGACY: [[u8; 3]; 23] = [
    [0, 0, 0],
    [0, 0, 0],
    [106, 127, 63],
    [63, 127, 107],
    [127, 102, 63],
    [138, 86, 62],
    [143, 126, 95],
    [139, 109, 79],
    [102, 108, 111],
    [101, 176, 234],
    [195, 199, 218],
    [137, 90, 71],
    [58, 46, 36],
    [30, 30, 37],
    [102, 92, 59],
    [232, 156, 74],
    [115, 123, 107],
    [132, 123, 90],
    [129, 194, 224],
    [115, 132, 74],
    [198, 189, 181],
    [206, 173, 148],
    [148, 148, 140],
];
#[rustfmt::skip]
pub(super) const BASE_2022: [[u8; 3]; 23] = [
    [0, 0, 0],
    [0, 0, 0],
    [111, 126, 62],
    [88, 89, 86],
    [152, 152, 152],
    [138, 97, 73],
    [207, 203, 167],
    [172, 148, 108],
    [99, 100, 102],
    [221, 228, 229],
    [235, 253, 255],
    [148, 124, 95],
    [121, 112, 98],
    [75, 74, 74],
    [140, 130, 104],
    [255, 24, 67],
    [80, 84, 84],
    [134, 134, 118],
    [204, 210, 223],
    [106, 134, 64],
    [255, 255, 254],
    [255, 243, 192],
    [143, 144, 135],
];
