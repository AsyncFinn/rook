//! The legacy text enums in terms of today's: each `Enum.Font` item as the
//! `FontFace` it stands for, and each `Enum.FontSize` item as a
//! `TextSize`. Files, scripts and layout all read them from here.

use rbx_types::{Font, FontStyle, FontWeight};

/// Where Roblox's font families live in its install.
pub const FAMILY_FOLDER: &str = "rbxasset://fonts/families/";

/// Each `Enum.Font` item's family file, weight and style, as
/// `Font.fromEnum` gives them (measured in Studio 0.743, every item).
/// `Unknown` has none.
pub const LEGACY_FONTS: &[(&str, &str, FontWeight, FontStyle)] = {
    use FontStyle::{Italic, Normal};
    use FontWeight::{Bold, ExtraBold, Heavy, Light, Medium, Regular, SemiBold};
    &[
        ("Legacy", "LegacyArial", Regular, Normal),
        ("Arial", "Arial", Regular, Normal),
        ("ArialBold", "Arial", Bold, Normal),
        ("SourceSans", "SourceSansPro", Regular, Normal),
        ("SourceSansBold", "SourceSansPro", Bold, Normal),
        ("SourceSansLight", "SourceSansPro", Light, Normal),
        ("SourceSansItalic", "SourceSansPro", Regular, Italic),
        ("Bodoni", "AccanthisADFStd", Regular, Normal),
        ("Garamond", "Guru", Regular, Normal),
        ("Cartoon", "ComicNeueAngular", Regular, Normal),
        ("Code", "Inconsolata", Regular, Normal),
        ("Highway", "HighwayGothic", Regular, Normal),
        ("SciFi", "Zekton", Regular, Normal),
        ("Arcade", "PressStart2P", Regular, Normal),
        ("Fantasy", "Balthazar", Regular, Normal),
        ("Antique", "RomanAntique", Regular, Normal),
        ("SourceSansSemibold", "SourceSansPro", SemiBold, Normal),
        ("Gotham", "GothamSSm", Regular, Normal),
        ("GothamMedium", "GothamSSm", Medium, Normal),
        ("GothamBold", "GothamSSm", Bold, Normal),
        ("GothamBlack", "GothamSSm", Heavy, Normal),
        ("AmaticSC", "AmaticSC", Regular, Normal),
        ("Bangers", "Bangers", Regular, Normal),
        ("Creepster", "Creepster", Regular, Normal),
        ("DenkOne", "DenkOne", Regular, Normal),
        ("Fondamento", "Fondamento", Regular, Normal),
        ("FredokaOne", "FredokaOne", Regular, Normal),
        ("GrenzeGotisch", "GrenzeGotisch", Regular, Normal),
        ("IndieFlower", "IndieFlower", Regular, Normal),
        ("JosefinSans", "JosefinSans", Regular, Normal),
        ("Jura", "Jura", Regular, Normal),
        ("Kalam", "Kalam", Regular, Normal),
        ("LuckiestGuy", "LuckiestGuy", Regular, Normal),
        ("Merriweather", "Merriweather", Regular, Normal),
        ("Michroma", "Michroma", Regular, Normal),
        ("Nunito", "Nunito", Regular, Normal),
        ("Oswald", "Oswald", Regular, Normal),
        ("PatrickHand", "PatrickHand", Regular, Normal),
        ("PermanentMarker", "PermanentMarker", Regular, Normal),
        ("Roboto", "Roboto", Regular, Normal),
        ("RobotoCondensed", "RobotoCondensed", Regular, Normal),
        ("RobotoMono", "RobotoMono", Regular, Normal),
        ("Sarpanch", "Sarpanch", Regular, Normal),
        ("SpecialElite", "SpecialElite", Regular, Normal),
        ("TitilliumWeb", "TitilliumWeb", Regular, Normal),
        ("Ubuntu", "Ubuntu", Regular, Normal),
        ("BuilderSans", "BuilderSans", Regular, Normal),
        ("BuilderSansMedium", "BuilderSans", Medium, Normal),
        ("BuilderSansBold", "BuilderSans", Bold, Normal),
        ("BuilderSansExtraBold", "BuilderSans", ExtraBold, Normal),
        ("Arimo", "Arimo", Regular, Normal),
        ("ArimoBold", "Arimo", Bold, Normal),
    ]
};

/// The `FontFace` an `Enum.Font` item stands for.
pub fn font_from_enum(item: &str) -> Option<Font> {
    LEGACY_FONTS
        .iter()
        .find(|(name, ..)| *name == item)
        .map(|&(_, family, weight, style)| {
            Font::new(&format!("{FAMILY_FOLDER}{family}.json"), weight, style)
        })
}

/// The `Enum.Font` item a `FontFace` is, `"Unknown"` when it's none of
/// them (Inference: Studio reads `Font` as Unknown for other faces).
pub fn enum_of_font(font: &Font) -> &'static str {
    LEGACY_FONTS
        .iter()
        .find(|&&(_, family, weight, style)| {
            font.weight == weight
                && font.style == style
                && font.family.strip_prefix(FAMILY_FOLDER) == Some(&format!("{family}.json"))
        })
        .map_or("Unknown", |(name, ..)| name)
}

/// Each `Enum.FontSize` item's `TextSize`.
pub const FONT_SIZES: &[(&str, f32)] = &[
    ("Size8", 8.0),
    ("Size9", 9.0),
    ("Size10", 10.0),
    ("Size11", 11.0),
    ("Size12", 12.0),
    ("Size14", 14.0),
    ("Size18", 18.0),
    ("Size24", 24.0),
    ("Size36", 36.0),
    ("Size48", 48.0),
    ("Size28", 28.0),
    ("Size32", 32.0),
    ("Size42", 42.0),
    ("Size60", 60.0),
    ("Size96", 96.0),
];

/// The `TextSize` of an `Enum.FontSize` item.
pub fn text_size_of(item: &str) -> Option<f32> {
    FONT_SIZES.iter().find(|(n, _)| *n == item).map(|&(_, s)| s)
}

/// The `Enum.FontSize` item a `TextSize` reads as: the smallest no smaller
/// than it (measured: 13 reads Size14), or the biggest.
pub fn font_size_of(text_size: f32) -> &'static str {
    FONT_SIZES
        .iter()
        .filter(|&&(_, s)| s >= text_size)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or("Size96", |(n, _)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_round_trips() {
        for (name, ..) in LEGACY_FONTS {
            let font = font_from_enum(name).unwrap();
            assert_eq!(enum_of_font(&font), *name);
        }
        let other = Font::new(
            "rbxassetid://12187365364",
            FontWeight::Regular,
            FontStyle::Normal,
        );
        assert_eq!(enum_of_font(&other), "Unknown");
        assert_eq!(font_size_of(13.0), "Size14");
        assert_eq!(font_size_of(500.0), "Size96");
        assert_eq!(text_size_of("Size24"), Some(24.0));
    }
}
