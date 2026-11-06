//! Built-in materials' textures, fetched from Roblox like any other asset.
//! Each material's textures are a TexturePack asset (asset type 63): a small
//! XML file naming one image asset per map (`color`, `normal`, `metalness`,
//! `roughness`). Which pack is which material's, and how that was found, is
//! in `docs/behavior/rendering/materials.md`.

use rook_types::{Material, MaterialSet};

use crate::MaterialMap;
use crate::decode::image::{Encoding, Image, ImageError, ImageFormat, MAX_SIZE};

/// Each textured material's pack in the legacy set, and the pack its normal
/// map comes from when it isn't the same one.
#[rustfmt::skip]
const LEGACY: &[(Material, u64, Option<u64>)] = &[
    (Material::Snow, 9439632387, None),
    (Material::Sandstone, 9873521380, None),
    (Material::Wood, 7547303225, None),
    (Material::WoodPlanks, 7547332968, None),
    (Material::Marble, 7547177270, None),
    (Material::Slate, 7547298114, None),
    (Material::Concrete, 7546653951, None),
    (Material::Granite, 7547164710, None),
    (Material::Brick, 7546650097, None),
    (Material::Pebble, 7547291361, None),
    (Material::CorrodedMetal, 7547184629, None),
    (Material::DiamondPlate, 7547162198, None),
    (Material::Foil, 7546645012, None),
    (Material::Metal, 7547288171, None),
    (Material::Grass, 7547169285, None),
    (Material::Sand, 7547295153, None),
    (Material::Fabric, 7547101130, None),
    (Material::Ice, 7547171356, None),
    (Material::Cobblestone, 7546652947, None),
    (Material::Glass, 7547304948, None),
    (Material::Mud, 9873319819, None),
    (Material::Limestone, 9873289812, None),
    (Material::Basalt, 9873270487, None),
    (Material::Ground, 9439044431, None),
    (Material::LeafyGrass, 9873288083, None),
    (Material::Rock, 9873515198, None),
    (Material::Pavement, 9873322398, None),
    (Material::Glacier, 9438851661, None),
    (Material::CrackedLava, 9438582231, None),
    (Material::Salt, 9439566986, None),
    (Material::Asphalt, 9873267379, None),
    (Material::Cardboard, 14108654559, None),
    (Material::Carpet, 14108663921, None),
    (Material::CeramicTiles, 17429458668, None),
    (Material::ClayRoofTiles, 18147685875, None),
    (Material::RoofShingles, 91579661064923, None),
    (Material::Leather, 14108671010, None),
    (Material::Plaster, 14108672569, None),
    (Material::Rubber, 14108675388, None),
];

/// The same for the 2022 set. Marble's, DiamondPlate's, Foil's and Glass's
/// are later packs that look the same, not the ones Studio fetches, which
/// weren't found (an Inference, in `docs/behavior/rendering/materials.md`).
#[rustfmt::skip]
const MATERIALS_2022: &[(Material, u64, Option<u64>)] = &[
    (Material::Snow, 9920620451, None),
    (Material::Sandstone, 9920596353, None),
    (Material::Wood, 9920625499, None),
    (Material::WoodPlanks, 9920626896, None),
    (Material::Marble, 104749709354369, None),
    (Material::Slate, 9920600052, None),
    (Material::Concrete, 9920484334, None),
    (Material::Granite, 9920550720, None),
    (Material::Brick, 9920482992, None),
    (Material::Pebble, 9920581197, None),
    (Material::CorrodedMetal, 9920589512, None),
    (Material::DiamondPlate, 124320368131155, None),
    (Material::Foil, 110583221457338, Some(136319143816079)),
    (Material::Metal, 9920574966, None),
    (Material::Grass, 9920552044, None),
    (Material::Sand, 9920591862, None),
    (Material::Fabric, 9920517963, None),
    (Material::Ice, 9920556429, None),
    (Material::Cobblestone, 9919719550, None),
    (Material::Glass, 88609722573469, None),
    (Material::Mud, 9920578676, None),
    (Material::Limestone, 9920561624, None),
    (Material::Basalt, 9920482224, None),
    (Material::Ground, 9920554695, None),
    (Material::LeafyGrass, 9920558145, None),
    (Material::Rock, 10129366149, None),
    (Material::Pavement, 9920580094, None),
    (Material::Glacier, 9920518995, None),
    (Material::CrackedLava, 9920485426, None),
    (Material::Salt, 9920590478, None),
    (Material::Asphalt, 9930003180, None),
    (Material::Cardboard, 14108654559, None),
    (Material::Carpet, 14108663921, None),
    (Material::CeramicTiles, 17429458668, None),
    (Material::ClayRoofTiles, 18147685875, None),
    (Material::RoofShingles, 91579661064923, None),
    (Material::Leather, 14108671010, None),
    (Material::Plaster, 14108672569, None),
    (Material::Rubber, 14108675388, None),
];

/// The packs holding the legacy set's detail normal maps: normal-only, 512
/// × 512, fetched by Studio beside each material's own pack. How they lie
/// is `Material::normal_detail`.
const LEGACY_DETAIL: &[(Material, u64)] =
    &[(Material::Wood, 7547298786), (Material::Slate, 7547298323)];

/// The TexturePack asset holding `material`'s `map` in `set`, if it has
/// that map.
pub fn pack(material: Material, set: MaterialSet, map: MaterialMap) -> Option<u64> {
    if map == MaterialMap::DetailNormal {
        let detail = LEGACY_DETAIL.iter().find(|(m, _)| *m == material);
        return detail.filter(|_| set == MaterialSet::Legacy).map(|d| d.1);
    }
    let table = match set {
        MaterialSet::Legacy => LEGACY,
        MaterialSet::Materials2022 => MATERIALS_2022,
    };
    let &(_, main, normal) = table.iter().find(|(m, ..)| *m == material)?;
    Some(match map {
        MaterialMap::Normal => normal.unwrap_or(main),
        _ => main,
    })
}

/// What a TexturePack asset names: an image asset id per map.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TexturePack {
    pub color: Option<u64>,
    pub normal: Option<u64>,
    pub metalness: Option<u64>,
    pub roughness: Option<u64>,
    /// `<alphamode>` 2: the color map's alpha marks where the surface's
    /// color tints it (a brick's mortar keeps its own). Other packs' alpha
    /// means nothing to Studio's shaders.
    pub tint_mask: bool,
}

impl TexturePack {
    /// Read a TexturePack asset: `<roblox>` holding `<color>`, `<normal>`,
    /// `<metalness>` and `<roughness>` image ids and `<alphamode>` (and
    /// what Rook doesn't use: `<height>`, which terrain's packs name but
    /// Studio's terrain doesn't draw, `<usage>`, `<tiling>`,
    /// `<emissive...>`).
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(bytes).ok()?;
        if !text.trim_start().starts_with("<roblox") {
            return None;
        }
        let id = |tag: &str| {
            let open = format!("<{tag}>");
            let start = text.find(&open)? + open.len();
            let end = start + text[start..].find('<')?;
            text[start..end].trim().parse().ok()
        };
        Some(Self {
            color: id("color"),
            normal: id("normal"),
            metalness: id("metalness"),
            roughness: id("roughness"),
            tint_mask: id("alphamode") == Some(2),
        })
    }
}

/// A material map from its image files, block-compressed at its full size
/// (Roblox's are 1024 × 1024): the color map as BC1 (BC3 where it has
/// alpha), the normal maps as BC5 (BC1 and quick BC7 flatten their finest
/// relief, measured at `docs/behavior/rendering/materials.md#normal-maps`),
/// the roughness-metalness map as BC7.
/// A material's three maps hold 3.3 MB with their mips, where RGBA8 would
/// hold 16 MB (40 materials: 133 MB against 640). A color map keeps its alpha only
/// as a `tint_mask` ([`TexturePack::tint_mask`]); otherwise it's opaque,
/// so drawing tints it whole.
pub fn build(
    map: MaterialMap,
    main: &[u8],
    metalness: Option<&[u8]>,
    tint_mask: bool,
) -> Result<Image, ImageError> {
    let (mut rgba, format) = packed(map, main, metalness)?;
    if map == MaterialMap::Color && !tint_mask {
        for p in rgba.pixels_mut() {
            p[3] = 255;
        }
    }
    let image = Image::from_rgba(rgba, format, MAX_SIZE)?;
    let encoding = match map {
        MaterialMap::Color if image.translucent => Encoding::Bc3,
        MaterialMap::Color => Encoding::Bc1,
        MaterialMap::Normal | MaterialMap::DetailNormal => Encoding::Bc5,
        MaterialMap::RoughnessMetalness => Encoding::Bc7,
    };
    Ok(image.compressed(encoding))
}

/// A material map's pixels: the color or normal map as it is (normals
/// are ordinary RGB normal maps), or roughness (into green), metalness
/// (into red, 0 when the pack has none) packed into one, with an opaque
/// alpha so mips aren't weighted by it.
fn packed(
    map: MaterialMap,
    main: &[u8],
    metalness: Option<&[u8]>,
) -> Result<(::image::RgbaImage, ImageFormat), ImageError> {
    let (mut rgba, format) = crate::decode::image::decode_rgba(main)?;
    if map == MaterialMap::RoughnessMetalness {
        let metal = match metalness {
            Some(bytes) => {
                let (m, _) = crate::decode::image::decode_rgba(bytes)?;
                let (w, h) = rgba.dimensions();
                let filter = ::image::imageops::FilterType::Triangle;
                Some(::image::imageops::resize(&m, w, h, filter))
            }
            None => None,
        };
        for (x, y, p) in rgba.enumerate_pixels_mut() {
            let m = metal.as_ref().map_or(0, |m| m.get_pixel(x, y)[0]);
            p.0 = [m, p[0], 0, 255];
        }
    }
    Ok((rgba, format))
}

/// Every pack id in the tables (tests and tools).
pub fn all_packs() -> impl Iterator<Item = u64> {
    LEGACY
        .iter()
        .chain(MATERIALS_2022)
        .flat_map(|&(_, main, normal)| std::iter::once(main).chain(normal))
        .chain(LEGACY_DETAIL.iter().map(|d| d.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_texture_packs() {
        let xml = b"<roblox>\n  <texturepack_version>2</texturepack_version>\n  <usage>0</usage>\n  <color>11599917010</color>\n  <normal>11498586912</normal>\n  <metalness>11498586281</metalness>\n  <roughness>11498587586</roughness>\n</roblox>";
        assert_eq!(
            TexturePack::parse(xml),
            Some(TexturePack {
                color: Some(11599917010),
                normal: Some(11498586912),
                metalness: Some(11498586281),
                roughness: Some(11498587586),
                tint_mask: false,
            })
        );
        let normal_only = b"<roblox><usage>3</usage><normal>9438453869</normal></roblox>";
        assert_eq!(TexturePack::parse(normal_only).unwrap().color, None);
        // Legacy Brick's pack: its color map's alpha is a tint mask.
        let brick = b"<roblox><alphamode>2</alphamode><color>7546648254</color></roblox>";
        assert!(TexturePack::parse(brick).unwrap().tint_mask);
        let wood = b"<roblox><alphamode>1</alphamode><color>7547190453</color></roblox>";
        assert!(!TexturePack::parse(wood).unwrap().tint_mask);
        assert_eq!(TexturePack::parse(b"\x89PNG"), None);
    }

    #[test]
    fn tables() {
        use Material::*;
        assert_eq!(
            pack(Wood, MaterialSet::Legacy, MaterialMap::Color),
            Some(7547303225)
        );
        assert_eq!(
            pack(Wood, MaterialSet::Legacy, MaterialMap::Normal),
            Some(7547303225)
        );
        assert_eq!(
            pack(Wood, MaterialSet::Legacy, MaterialMap::DetailNormal),
            Some(7547298786)
        );
        assert_eq!(
            pack(Wood, MaterialSet::Materials2022, MaterialMap::DetailNormal),
            None
        );
        assert_eq!(
            pack(Brick, MaterialSet::Legacy, MaterialMap::DetailNormal),
            None
        );
        assert_eq!(
            pack(Plastic, MaterialSet::Materials2022, MaterialMap::Color),
            None
        );
        // Every textured material is in both sets, once.
        for table in [LEGACY, MATERIALS_2022] {
            assert_eq!(table.len(), 39);
            for (i, (m, ..)) in table.iter().enumerate() {
                assert!(!table[..i].iter().any(|(n, ..)| n == m), "{m:?} twice");
            }
        }
        assert_eq!(all_packs().count(), 39 * 2 + 3);
    }

    /// Roughness goes to green and metalness to red.
    #[test]
    fn packs_roughness_and_metalness() {
        let png = |v: u8| {
            let img = ::image::GrayImage::from_pixel(4, 4, ::image::Luma([v]));
            let mut out = Vec::new();
            img.write_to(
                &mut std::io::Cursor::new(&mut out),
                ::image::ImageFormat::Png,
            )
            .unwrap();
            out
        };
        let (rm, _) = packed(MaterialMap::RoughnessMetalness, &png(200), Some(&png(30))).unwrap();
        assert_eq!(rm.get_pixel(0, 0).0, [30, 200, 0, 255]);
        let (r, _) = packed(MaterialMap::RoughnessMetalness, &png(90), None).unwrap();
        assert_eq!(r.get_pixel(0, 0).0, [0, 90, 0, 255]);
        // Built, it's BC7.
        let built = build(MaterialMap::RoughnessMetalness, &png(90), None, false).unwrap();
        assert_eq!(built.encoding, Encoding::Bc7);
    }

    /// A color map keeps its alpha only as a tint mask.
    #[test]
    fn color_maps_keep_alpha_only_as_a_tint_mask() {
        let img = ::image::RgbaImage::from_pixel(4, 4, ::image::Rgba([200, 100, 50, 30]));
        let mut png = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png),
            ::image::ImageFormat::Png,
        )
        .unwrap();
        let masked = build(MaterialMap::Color, &png, None, true).unwrap();
        assert!(masked.translucent);
        assert_eq!(masked.encoding, Encoding::Bc3);
        let opaque = build(MaterialMap::Color, &png, None, false).unwrap();
        assert!(!opaque.translucent);
        assert_eq!(opaque.encoding, Encoding::Bc1);
    }
}
