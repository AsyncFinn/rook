//! Each terrain material's shape and textures, from Studio's
//! `FStringTerrainMaterialTablePre2022` and `FStringTerrainMaterialTable2022`
//! settings (Studio 0.742's `ClientSettings`), in `TerrainMaterial` order.
//! See [`Look`]. Shapes always come from the legacy table; textures from
//! the place's set's.

use super::{Edges, Look, Texture};

const fn tex(pack: u64, tiling: f32, detiling: f32, rotation: f32) -> Texture {
    Texture {
        pack,
        tiling,
        detiling,
        rotation,
    }
}

#[rustfmt::skip]
pub(super) const LOOKS: [Look; 23] = [
    // Air
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: None, side: None, bottom: None },
    // Water
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: None, side: None, bottom: None },
    // Grass
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547336797, 0.25, 4.0, 1.0)), side: Some(tex(7547623564, 0.2, 5.0, 0.0)), bottom: Some(tex(7551928998, 0.45, 2.2219, 0.0)) },
    // Slate
    Look { shift: 0.1, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547627210, 0.55, 1.818, 0.0)), side: Some(tex(7547627210, 0.55, 1.818, 0.0)), bottom: Some(tex(7547627210, 0.55, 1.818, 0.0)) },
    // Concrete
    Look { shift: 0.0, cubify: 0.0, quantize: 0.5, edges: Edges::Hard, cube: false, top: Some(tex(7547337977, 0.5, 1.25, 0.0)), side: Some(tex(7547630026, 0.4, 2.5, 0.0)), bottom: Some(tex(7547630026, 0.4, 2.5, 0.0)) },
    // Brick
    Look { shift: 0.0, cubify: 1.0, quantize: 0.0, edges: Edges::Hard, cube: true, top: Some(tex(7547633775, 0.8, 5.0, 0.0)), side: Some(tex(7547633775, 0.8, 5.0, 0.0)), bottom: Some(tex(7547633775, 0.8, 5.0, 0.0)) },
    // Sand
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547636223, 0.25, 4.0, 0.0)), side: Some(tex(7546659291, 0.25, 1.818, 0.0)), bottom: Some(tex(7546659291, 0.25, 1.818, 0.0)) },
    // WoodPlanks
    Look { shift: 0.0, cubify: 1.0, quantize: 0.0, edges: Edges::Hard, cube: true, top: Some(tex(7547642685, 0.8, 2.0, 0.0)), side: Some(tex(7547642685, 0.8, 2.0, 0.0)), bottom: Some(tex(7547642685, 0.8, 2.0, 0.0)) },
    // Rock
    Look { shift: 0.2998, cubify: 0.0, quantize: 0.0, edges: Edges::HardSoft, cube: false, top: Some(tex(7547645700, 0.55, 1.818, 0.5)), side: Some(tex(7547645700, 0.55, 1.818, 0.5)), bottom: Some(tex(7547645700, 0.55, 1.818, 0.5)) },
    // Glacier
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7551935562, 0.23, 1.818, 0.0)), side: Some(tex(7551944502, 0.2, 1.1108, 0.0)), bottom: Some(tex(7551949364, 0.23, 1.6653, 0.0)) },
    // Snow
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7551952124, 0.2998, 3.335_557, 0.0)), side: Some(tex(7551952124, 0.2998, 3.335_557, 0.0)), bottom: Some(tex(7551952124, 0.2998, 3.335_557, 0.0)) },
    // Sandstone
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7551956653, 0.5, 1.429592, 0.5)), side: Some(tex(7551965250, 0.2998, 2.0, 0.0)), bottom: Some(tex(7552020001, 0.45, 3.333, 0.0)) },
    // Mud
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7552025321, 0.2998, 10.0, 0.0)), side: Some(tex(7552025321, 0.2998, 10.0, 0.0)), bottom: Some(tex(7552025321, 0.2998, 10.0, 0.0)) },
    // Basalt
    Look { shift: 0.2, cubify: 0.0, quantize: 0.0, edges: Edges::HardSoft, cube: false, top: Some(tex(7551978967, 0.55, 1.818, 0.0)), side: Some(tex(7551978967, 0.55, 1.818, 0.0)), bottom: Some(tex(7551978967, 0.55, 1.818, 0.0)) },
    // Ground
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547349135, 0.2998, 3.333, 0.5)), side: Some(tex(7547349135, 0.2998, 3.333, 0.5)), bottom: Some(tex(7547349135, 0.2998, 3.333, 0.5)) },
    // CrackedLava
    Look { shift: 0.1, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7551983669, 0.28, 3.333, 0.5)), side: Some(tex(7551983669, 0.28, 3.333, 0.5)), bottom: Some(tex(7551983669, 0.28, 3.333, 0.5)) },
    // Asphalt
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7551987784, 0.25, 4.3478, 0.0)), side: Some(tex(7551990829, 0.2, 1.1108, 0.0)), bottom: Some(tex(7551990829, 0.2, 1.1108, 0.0)) },
    // Cobblestone
    Look { shift: 0.0, cubify: 0.0, quantize: 0.1499, edges: Edges::HardSoft, cube: false, top: Some(tex(7551995071, 0.23, 4.0, 0.0)), side: Some(tex(7547650812, 0.2, 1.1108, 0.0)), bottom: Some(tex(7547650812, 0.2, 1.1108, 0.0)) },
    // Ice
    Look { shift: 0.1499, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547655101, 0.25, 4.0, 0.1)), side: Some(tex(7547659308, 0.25, 4.0, 0.0)), bottom: Some(tex(7547659308, 0.25, 4.0, 0.0)) },
    // LeafyGrass
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7546664715, 0.25, 3.0299, 1.0)), side: Some(tex(7546666342, 0.25, 4.0, 0.0)), bottom: Some(tex(7546666342, 0.25, 4.0, 0.0)) },
    // Salt
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547663777, 0.23, 1.818, 0.0)), side: Some(tex(7547669153, 0.2, 1.1108, 0.0)), bottom: Some(tex(7547669153, 0.2, 1.1108, 0.0)) },
    // Limestone
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(7547673056, 0.25, 1.818, 0.1)), side: Some(tex(7547676869, 0.25, 2.0, 0.0)), bottom: Some(tex(7547676869, 0.25, 2.0, 0.0)) },
    // Pavement
    Look { shift: 0.0, cubify: 0.0, quantize: 0.25, edges: Edges::HardSoft, cube: false, top: Some(tex(7547679257, 0.46, 4.0, 0.0)), side: Some(tex(7547684004, 0.46, 4.0, 0.0)), bottom: Some(tex(7547684004, 0.46, 4.0, 0.0)) },
];

/// The 2022 set's textures (its shape numbers aren't used).
#[rustfmt::skip]
pub(super) const LOOKS_2022: [Look; 23] = [
    // Air
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: None, side: None, bottom: None },
    // Water
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: None, side: None, bottom: None },
    // Grass
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920552044, 0.5, 3.717, 0.5)), side: Some(tex(9932815429, 0.5, 5.0, 0.0)), bottom: Some(tex(9933024903, 0.5, 2.222, 0.0)) },
    // Slate
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920600052, 0.5, 1.292, 0.0)), side: Some(tex(9920614301, 0.5, 2.222, 0.0)), bottom: Some(tex(9920614301, 0.5, 2.222, 0.0)) },
    // Concrete
    Look { shift: 0.0, cubify: 0.0, quantize: 1.0, edges: Edges::Hard, cube: false, top: Some(tex(9920484334, 0.5, 3.134, 0.0)), side: Some(tex(9920484334, 0.5, 3.134, 0.0)), bottom: Some(tex(9920484334, 0.5, 3.134, 0.0)) },
    // Brick
    Look { shift: 0.0, cubify: 1.0, quantize: 0.0, edges: Edges::Hard, cube: true, top: Some(tex(9920482992, 0.5, 1.0, 0.0)), side: Some(tex(9920482992, 0.5, 1.0, 0.0)), bottom: Some(tex(9920482992, 0.5, 1.0, 0.0)) },
    // Sand
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920591862, 0.5, 3.793, 0.0)), side: Some(tex(9920592389, 0.5, 10.0, 0.0)), bottom: Some(tex(9920592389, 0.5, 10.0, 0.0)) },
    // WoodPlanks
    Look { shift: 0.0, cubify: 1.0, quantize: 0.0, edges: Edges::Hard, cube: true, top: Some(tex(9920626896, 0.5, 1.0, 0.0)), side: Some(tex(9920626896, 0.5, 1.0, 0.0)), bottom: Some(tex(9920626896, 0.5, 1.0, 0.0)) },
    // Rock
    Look { shift: 0.299, cubify: 0.0, quantize: 0.0, edges: Edges::HardSoft, cube: false, top: Some(tex(10129366149, 0.5, 1.73, 0.0)), side: Some(tex(10129366149, 0.5, 1.73, 0.0)), bottom: Some(tex(10129366149, 0.5, 1.73, 0.0)) },
    // Glacier
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920518995, 0.5, 6.0, 0.0)), side: Some(tex(9920520478, 0.5, 6.0, 0.0)), bottom: Some(tex(9920521186, 0.5, 6.0, 0.0)) },
    // Snow
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920620451, 0.5, 2.73, 0.0)), side: Some(tex(9920621070, 0.5, 3.0, 0.0)), bottom: Some(tex(9920621070, 0.5, 3.0, 0.0)) },
    // Sandstone
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920596353, 0.5, 7.222, 0.0)), side: Some(tex(9920628394, 0.5, 2.222, 0.0)), bottom: Some(tex(9920628394, 0.5, 2.222, 0.0)) },
    // Mud
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920578676, 0.5, 1.3, 0.0)), side: Some(tex(9920578676, 0.5, 1.3, 0.0)), bottom: Some(tex(9920578676, 0.5, 1.3, 0.0)) },
    // Basalt
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::HardSoft, cube: false, top: Some(tex(9920482224, 0.5, 1.0, 0.0)), side: Some(tex(9920482224, 0.5, 1.0, 0.0)), bottom: Some(tex(9920482224, 0.5, 1.0, 0.0)) },
    // Ground
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920554695, 0.5, 2.79, 0.0)), side: Some(tex(9920554695, 0.5, 2.79, 0.0)), bottom: Some(tex(9920554695, 0.5, 2.79, 0.0)) },
    // CrackedLava
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920485426, 0.5, 2.0, 0.0)), side: Some(tex(9920485426, 0.5, 2.0, 0.0)), bottom: Some(tex(9920485426, 0.5, 2.0, 0.0)) },
    // Asphalt
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9930003180, 0.5, 2.898, 0.0)), side: Some(tex(9930005889, 0.5, 2.898, 0.0)), bottom: Some(tex(9930005889, 0.5, 2.898, 0.0)) },
    // Cobblestone
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::HardSoft, cube: false, top: Some(tex(9919719550, 0.5, 1.0, 0.0)), side: Some(tex(9919719550, 0.5, 1.0, 0.0)), bottom: Some(tex(9919719550, 0.5, 1.0, 0.0)) },
    // Ice
    Look { shift: 0.149, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920556429, 0.5, 3.78, 0.0)), side: Some(tex(9920556887, 0.5, 8.0, 0.0)), bottom: Some(tex(9920556887, 0.5, 8.0, 0.0)) },
    // LeafyGrass
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920558145, 0.5, 3.793, 0.0)), side: Some(tex(9933025110, 0.5, 3.71, 0.0)), bottom: Some(tex(9933025110, 0.5, 3.71, 0.0)) },
    // Salt
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920590478, 0.5, 2.0, 0.0)), side: Some(tex(9920590955, 0.5, 5.0, 0.0)), bottom: Some(tex(9920590955, 0.5, 5.0, 0.0)) },
    // Limestone
    Look { shift: 0.0, cubify: 0.0, quantize: 0.0, edges: Edges::Soft, cube: false, top: Some(tex(9920561624, 0.5, 6.0, 0.0)), side: Some(tex(9920562806, 0.5, 6.0, 0.0)), bottom: Some(tex(9920562806, 0.5, 6.0, 0.0)) },
    // Pavement
    Look { shift: 0.0, cubify: 0.0, quantize: 0.25, edges: Edges::HardSoft, cube: false, top: Some(tex(9920580094, 0.5, 1.0, 0.0)), side: Some(tex(9920580503, 0.5, 6.0, 0.0)), bottom: Some(tex(9920580503, 0.5, 6.0, 0.0)) },
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
