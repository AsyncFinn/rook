//! Built-in materials' textures, fetched from Roblox like any other asset
//! (`docs/behavior/materials.md`).
//!
//! Each material's textures are a TexturePack asset (asset type 63): a
//! small XML file naming one image asset per map (`color`, `normal`,
//! `metalness`, `roughness`). Asset delivery serves both anonymously, and
//! they go through the disk cache like everything else.
//!
//! Which pack is which material's comes from Studio itself: its
//! `PartMaterialInfosPre2022` and `PartMaterialInfos2022` tables (Studio
//! 0.743), whose ids are listed in the order of the Material values next
//! to them. That order was checked against the textures Studio showed for
//! each material (identified in its content cache in M4 step 5) by
//! comparing pictures. Most materials' normal maps came out the same image
//! (differences under 1.5 of 255), which pins every pack to its material.
//! For legacy Wood and Slate and 2022 Foil, Studio showed the normal of a
//! separate normal-only pack in the same table; those are listed as each
//! one's normal pack. Plastic, SmoothPlastic, Neon and ForceField have no
//! textures here, as in Studio.

use rook_types::{Material, MaterialSet};

use crate::MaterialMap;
use crate::decode::image::{Image, ImageError};

/// Material textures are kept at 512 × 512 at most (Roblox's are 1024): 40
/// materials' three maps at 1024 would hold 640 MB.
pub const MATERIAL_MAX_SIZE: u32 = 512;

/// Each textured material's pack in the legacy set, and the pack its normal
/// map comes from when it isn't the same one.
#[rustfmt::skip]
const LEGACY: &[(Material, u64, Option<u64>)] = &[
    (Material::Snow, 9439632387, None),
    (Material::Sandstone, 9873521380, None),
    (Material::Wood, 7547303225, Some(7547298786)),
    (Material::WoodPlanks, 7547332968, None),
    (Material::Marble, 7547177270, None),
    (Material::Slate, 7547298114, Some(7547298323)),
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

/// The same for the 2022 set.
#[rustfmt::skip]
const MATERIALS_2022: &[(Material, u64, Option<u64>)] = &[
    (Material::Snow, 106058971152252, None),
    (Material::Sandstone, 109869731739139, None),
    (Material::Wood, 73489845523797, None),
    (Material::WoodPlanks, 78212019144684, None),
    (Material::Marble, 104749709354369, None),
    (Material::Slate, 130174615834794, None),
    (Material::Concrete, 115089551732435, None),
    (Material::Granite, 125518382244973, None),
    (Material::Brick, 85077012310177, None),
    (Material::Pebble, 87820215580582, None),
    (Material::CorrodedMetal, 122577965407434, None),
    (Material::DiamondPlate, 124320368131155, None),
    (Material::Foil, 110583221457338, Some(136319143816079)),
    (Material::Metal, 71003343637114, None),
    (Material::Grass, 106637943980559, None),
    (Material::Sand, 117101875286338, None),
    (Material::Fabric, 132264823269423, None),
    (Material::Ice, 91952184132897, None),
    (Material::Cobblestone, 129616831094994, None),
    (Material::Glass, 88609722573469, None),
    (Material::Mud, 81832637858593, None),
    (Material::Limestone, 136792039489433, None),
    (Material::Basalt, 103124999245988, None),
    (Material::Ground, 122356188009715, None),
    (Material::LeafyGrass, 75665810846977, None),
    (Material::Rock, 112102163340384, None),
    (Material::Pavement, 100821413010093, None),
    (Material::Glacier, 85752927082668, None),
    (Material::CrackedLava, 121276091081821, None),
    (Material::Salt, 137066264708856, None),
    (Material::Asphalt, 77679553820203, None),
    (Material::Cardboard, 112994935967349, None),
    (Material::Carpet, 119478636506120, None),
    (Material::CeramicTiles, 126380627739075, None),
    (Material::ClayRoofTiles, 114168170326154, None),
    (Material::RoofShingles, 107847976513485, None),
    (Material::Leather, 111918862958038, None),
    (Material::Plaster, 104555660139073, None),
    (Material::Rubber, 112374357118714, None),
];

/// The TexturePack asset holding `material`'s `map` in `set`, if it has
/// textures.
pub fn pack(material: Material, set: MaterialSet, map: MaterialMap) -> Option<u64> {
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
    /// How high each point of the surface stands (terrain's packs: where
    /// materials meet, the higher one shows).
    pub height: Option<u64>,
}

impl TexturePack {
    /// Read a TexturePack asset: `<roblox>` holding `<color>`, `<normal>`,
    /// `<metalness>`, `<roughness>` and `<height>` image ids (and settings
    /// Rook doesn't use: `<usage>`, `<alphamode>`, `<tiling>`,
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
            height: id("height"),
        })
    }
}

/// A material map from its image files: the colour or normal map as it is
/// (normals are ordinary RGB normal maps), or roughness (into green),
/// metalness (into red, 0 when the pack has none) and height (into blue,
/// full when it has none) packed into one, with an opaque alpha so mips
/// aren't weighted by it.
pub fn build(
    map: MaterialMap,
    main: &[u8],
    metalness: Option<&[u8]>,
    height: Option<&[u8]>,
) -> Result<Image, ImageError> {
    let (mut rgba, format) = crate::decode::image::decode_rgba(main)?;
    if map == MaterialMap::RoughnessMetalness {
        let (w, h) = rgba.dimensions();
        let sized = |bytes: Option<&[u8]>| -> Result<_, ImageError> {
            let Some(bytes) = bytes else {
                return Ok(None);
            };
            let (m, _) = crate::decode::image::decode_rgba(bytes)?;
            let filter = ::image::imageops::FilterType::Triangle;
            Ok(Some(::image::imageops::resize(&m, w, h, filter)))
        };
        let metal = sized(metalness)?;
        let height = sized(height)?;
        for (x, y, p) in rgba.enumerate_pixels_mut() {
            let m = metal.as_ref().map_or(0, |m| m.get_pixel(x, y)[0]);
            let h = height.as_ref().map_or(255, |h| h.get_pixel(x, y)[0]);
            p.0 = [m, p[0], h, 255];
        }
    }
    Image::from_rgba(rgba, format, MATERIAL_MAX_SIZE)
}

/// Every pack id in the tables (tests and tools).
pub fn all_packs() -> impl Iterator<Item = u64> {
    LEGACY
        .iter()
        .chain(MATERIALS_2022)
        .flat_map(|&(_, main, normal)| std::iter::once(main).chain(normal))
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
                height: None,
            })
        );
        let normal_only = b"<roblox><usage>3</usage><normal>9438453869</normal></roblox>";
        assert_eq!(TexturePack::parse(normal_only).unwrap().color, None);
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
            Some(7547298786)
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
        let rm = build(
            MaterialMap::RoughnessMetalness,
            &png(200),
            Some(&png(30)),
            Some(&png(70)),
        )
        .unwrap();
        assert_eq!(&rm.mips[0][..4], &[30, 200, 70, 255]);
        let r = build(MaterialMap::RoughnessMetalness, &png(90), None, None).unwrap();
        assert_eq!(&r.mips[0][..4], &[0, 90, 255, 255]);
    }
}
