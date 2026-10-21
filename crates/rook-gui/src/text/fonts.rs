//! Fonts: the family file a `FontFace` names, the face in it nearest the
//! weight and style asked for, and that face's file, loaded once.
//! Families and faces in the install load at once; ones by asset id are
//! fetched by the engine ([`Fonts::take_requests`]), and text waits for
//! them in the default family's face (docs/behavior/gui/text.md#fonts).

use std::sync::Arc;

use harfrust::{FontRef, ShaperData};
use rook_content::{ContentRef, Resolver};
use rook_types::{FAMILY_FOLDER, Font, FontStyle};
use rustc_hash::FxHashMap;
use serde::Deserialize;
use skrifa::raw::TableProvider;

/// A loaded face's index in [`Fonts`].
pub(crate) type FaceId = u32;

/// Families whose file the install doesn't have, and the one Studio draws
/// them with (measured: Arial and Arimo, Gotham and Montserrat give the
/// same `TextBounds` at every size tried).
const STAND_INS: &[(&str, &str)] = &[
    ("Arial.json", "Arimo.json"),
    ("GothamSSm.json", "Montserrat.json"),
];

/// What text falls back to while its face loads, or when it can't.
const FALLBACK_FAMILY: &str = "rbxasset://fonts/families/SourceSansPro.json";

/// Measured: Legacy draws Arimo half as big again as `TextSize` (its
/// `TextBounds` at 24 are Arial's at 36, at every size from 1 to 100).
const LEGACY_FAMILY: &str = "LegacyArial.json";
const LEGACY_SCALE: f32 = 1.5;

/// One font file, ready to shape and draw.
pub(crate) struct Face {
    data: Arc<[u8]>,
    pub(crate) shaper: ShaperData,
    pub(crate) upm: f32,
    /// The hhea ascent, and ascent less descent, in font units. Measured:
    /// a line of text is `TextSize` tall, and that height is the face's
    /// ascent to descent (Source Sans and Arimo widths fit only that).
    pub(crate) ascent: f32,
    pub(crate) line: f32,
}

impl std::fmt::Debug for Face {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Face")
            .field("upm", &self.upm)
            .finish_non_exhaustive()
    }
}

impl Face {
    fn load(data: Arc<[u8]>) -> Option<Self> {
        let font = FontRef::new(&data).ok()?;
        let upm = f32::from(font.head().ok()?.units_per_em());
        let hhea = font.hhea().ok()?;
        let ascent = f32::from(hhea.ascender().to_i16());
        let line = ascent - f32::from(hhea.descender().to_i16());
        let shaper = ShaperData::new(&font);
        (upm > 0.0 && line > 0.0).then_some(Self {
            data,
            shaper,
            upm,
            ascent,
            line,
        })
    }

    pub(crate) fn font(&self) -> FontRef<'_> {
        FontRef::new(&self.data).expect("checked when loaded")
    }
}

/// A face listed in a family file.
#[derive(Debug, Clone, Deserialize)]
struct Listed {
    weight: u16,
    style: String,
    #[serde(rename = "assetId")]
    asset: String,
}

#[derive(Debug, Deserialize)]
struct FamilyFile {
    faces: Vec<Listed>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Loaded(FaceId),
    Waiting,
    Missing,
}

/// A face picked for some text, and how much bigger than `TextSize` it
/// draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Picked {
    pub(crate) face: FaceId,
    pub(crate) scale: f32,
}

#[derive(Debug, Default)]
pub(crate) struct Fonts {
    resolver: Option<Arc<Resolver>>,
    families: FxHashMap<String, Option<Vec<Listed>>>,
    faces: Vec<Face>,
    files: FxHashMap<String, Slot>,
    /// Families and face files by asset id, waiting to be fetched.
    requests: Vec<String>,
    generation: u64,
}

impl Fonts {
    pub(crate) fn set_resolver(&mut self, resolver: Arc<Resolver>) {
        self.resolver = Some(resolver);
    }

    /// Changes whenever a fetched font arrives.
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn face(&self, id: FaceId) -> &Face {
        &self.faces[id as usize]
    }

    /// Asset ids to fetch, each asked for once.
    pub(crate) fn take_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.requests)
    }

    /// A fetched family or face file (`None`: it failed). Whether it was
    /// one this was waiting for.
    pub(crate) fn arrived(&mut self, url: &str, bytes: Option<Arc<[u8]>>) -> bool {
        let mut used = false;
        if let Some(entry @ None) = self.families.get_mut(url) {
            *entry = bytes.as_deref().and_then(parse_family);
            used = true;
        }
        if self.files.get(url) == Some(&Slot::Waiting) {
            let slot = match bytes.and_then(Face::load) {
                Some(face) => {
                    self.faces.push(face);
                    Slot::Loaded(self.faces.len() as FaceId - 1)
                }
                None => Slot::Missing,
            };
            self.files.insert(url.to_owned(), slot);
            used = true;
        }
        self.generation += u64::from(used);
        used
    }

    /// The face to draw `font` with: its own when loaded, otherwise the
    /// default family's, otherwise none.
    pub(crate) fn pick(&mut self, font: &Font) -> Option<Picked> {
        let scale = if font.family.ends_with(LEGACY_FAMILY) {
            LEGACY_SCALE
        } else {
            1.0
        };
        let own = self.pick_in(&font.family, font);
        let face = own.or_else(|| self.pick_in(FALLBACK_FAMILY, font))?;
        Some(Picked { face, scale })
    }

    fn pick_in(&mut self, family: &str, font: &Font) -> Option<FaceId> {
        let url = stand_in(family);
        let listed = self.family(&url)?;
        let want_italic = font.style == FontStyle::Italic;
        let file = nearest(&listed, font.weight.as_u16(), want_italic)?
            .asset
            .clone();
        match self.file(&file) {
            Slot::Loaded(id) => Some(id),
            Slot::Waiting | Slot::Missing => None,
        }
    }

    fn family(&mut self, url: &str) -> Option<Vec<Listed>> {
        if let Some(known) = self.families.get(url) {
            return known.clone();
        }
        let local = is_local(url);
        let parsed = if local {
            self.read_local(url).as_deref().and_then(parse_family)
        } else {
            self.requests.push(url.to_owned());
            None
        };
        self.families.insert(url.to_owned(), parsed.clone());
        parsed
    }

    fn file(&mut self, url: &str) -> Slot {
        if let Some(&slot) = self.files.get(url) {
            return slot;
        }
        let slot = if is_local(url) {
            match self.read_local(url).and_then(Face::load) {
                Some(face) => {
                    self.faces.push(face);
                    Slot::Loaded(self.faces.len() as FaceId - 1)
                }
                None => Slot::Missing,
            }
        } else {
            self.requests.push(url.to_owned());
            Slot::Waiting
        };
        self.files.insert(url.to_owned(), slot);
        slot
    }

    fn read_local(&self, url: &str) -> Option<Arc<[u8]>> {
        let r = ContentRef::parse(url)?;
        Some(self.resolver.as_ref()?.resolve_local(&r)?.bytes)
    }
}

fn is_local(url: &str) -> bool {
    url.starts_with("rbxasset://")
}

/// The family file Studio really reads for `url`.
fn stand_in(url: &str) -> String {
    for (missing, used) in STAND_INS {
        if url.strip_prefix(FAMILY_FOLDER) == Some(missing) {
            return format!("{FAMILY_FOLDER}{used}");
        }
    }
    url.to_owned()
}

fn parse_family(bytes: &[u8]) -> Option<Vec<Listed>> {
    let file: FamilyFile = serde_json::from_slice(bytes).ok()?;
    (!file.faces.is_empty()).then_some(file.faces)
}

/// The listed face nearest `weight` in the wanted style, by CSS's rules
/// (Inference: Roblox's own choice isn't measured): the right style if
/// there is one; for a weight above 500 the next heavier, else the next
/// lighter; below 400 the reverse; 400 tries 500 first.
fn nearest(listed: &[Listed], weight: u16, italic: bool) -> Option<&Listed> {
    let styled: Vec<&Listed> = listed
        .iter()
        .filter(|f| (f.style == "italic") == italic)
        .collect();
    let pool = if styled.is_empty() {
        listed.iter().collect()
    } else {
        styled
    };
    let w = i32::from(weight);
    pool.into_iter().min_by_key(|f| {
        let fw = i32::from(f.weight);
        let preferred = match w {
            400 => fw == 400 || fw == 500 || fw < 400,
            _ if w > 500 => fw >= w,
            _ => fw <= w,
        };
        (!preferred, (fw - w).abs(), fw < w && w > 500)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(weight: u16, style: &str) -> Listed {
        Listed {
            weight,
            style: style.into(),
            asset: format!("{weight}{style}"),
        }
    }

    #[test]
    fn nearest_face() {
        let faces = [
            listed(300, "normal"),
            listed(400, "normal"),
            listed(700, "normal"),
            listed(400, "italic"),
        ];
        let pick = |w, i| nearest(&faces, w, i).unwrap().asset.clone();
        assert_eq!(pick(400, false), "400normal");
        assert_eq!(pick(600, false), "700normal");
        assert_eq!(pick(200, false), "300normal");
        assert_eq!(pick(700, true), "400italic");
        assert_eq!(pick(500, false), "400normal");
    }

    #[test]
    fn stand_ins() {
        assert_eq!(
            stand_in("rbxasset://fonts/families/Arial.json"),
            "rbxasset://fonts/families/Arimo.json"
        );
        assert_eq!(stand_in("rbxassetid://5"), "rbxassetid://5");
    }
}
