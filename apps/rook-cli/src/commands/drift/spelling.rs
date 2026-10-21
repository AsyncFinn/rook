//! American spelling in everything Rook writes: code, comments, shaders,
//! scripts and docs. Roblox's API is American (`Color3`, `Neutral`), so
//! Rook's own words match it. Words are split at anything that isn't a
//! letter, and camelCase at its capitals, then lowercased.

use super::{Report, tracked};

/// British word parts that never occur in an American word, with their
/// American spelling: any word holding one is British.
const PARTS: &[(&str, &str)] = &[
    ("colour", "color"),
    ("centred", "centered"),
    ("centre", "center"),
    ("behaviour", "behavior"),
    ("neighbour", "neighbor"),
    ("favour", "favor"),
    ("honour", "honor"),
    ("flavour", "flavor"),
    ("labour", "labor"),
    ("harbour", "harbor"),
    ("rumour", "rumor"),
    ("vapour", "vapor"),
    ("armour", "armor"),
    ("grey", "gray"),
    ("licence", "license"),
    ("catalogue", "catalog"),
    ("artefact", "artifact"),
    ("aluminium", "aluminum"),
    ("defence", "defense"),
    ("offence", "offense"),
    ("manoeuvre", "maneuver"),
    ("whilst", "while"),
    ("amongst", "among"),
];

/// British whole words, with their American spelling.
const WORDS: &[(&str, &str)] = &[
    ("metre", "meter"),
    ("metres", "meters"),
    ("litre", "liter"),
    ("litres", "liters"),
    ("fibre", "fiber"),
    ("fibres", "fibers"),
    ("theatre", "theater"),
    ("calibre", "caliber"),
    ("spectre", "specter"),
    ("ageing", "aging"),
    ("programme", "program"),
    ("programmes", "programs"),
    ("analyse", "analyze"),
    ("analysed", "analyzed"),
    ("analysing", "analyzing"),
    ("travelled", "traveled"),
    ("travelling", "traveling"),
    ("cancelled", "canceled"),
    ("cancelling", "canceling"),
    ("modelled", "modeled"),
    ("modelling", "modeling"),
    ("labelled", "labeled"),
    ("labelling", "labeling"),
    ("levelled", "leveled"),
    ("levelling", "leveling"),
    ("signalled", "signaled"),
    ("signalling", "signaling"),
    ("channelled", "channeled"),
    ("totalled", "totaled"),
    ("fuelled", "fueled"),
    ("tunnelled", "tunneled"),
    ("marshalled", "marshaled"),
    ("fulfil", "fulfill"),
    ("enrol", "enroll"),
];

/// Stems that take -ise in British and -ize in American.
const ISE_STEMS: &[&str] = &[
    "normal",
    "optim",
    "recogn",
    "initial",
    "serial",
    "deserial",
    "organ",
    "final",
    "custom",
    "priorit",
    "minim",
    "maxim",
    "summar",
    "real",
    "visual",
    "synchron",
    "util",
    "author",
    "categor",
    "standard",
    "material",
    "special",
    "general",
    "quant",
    "random",
    "token",
    "parameter",
    "character",
    "stabil",
    "equal",
    "capital",
    "apolog",
    "critic",
    "memor",
    "digit",
    "raster",
    "vector",
    "linear",
    "discret",
    "sanit",
    "emphas",
    "symbol",
    "central",
    "local",
    "harmon",
    "rational",
    "steril",
    "polar",
    "trivial",
    "monet",
];

/// What follows an -ise stem.
const ISE_ENDINGS: &[&str] = &[
    "ise", "ised", "ises", "ising", "isation", "isations", "iser", "isers",
];

/// The files the check reads: everything Rook writes by hand.
const FILES: &[&str] = &[
    "*.md", "*.rs", "*.wgsl", "*.wesl", "*.luau", "*.lua", "*.toml", "*.ps1", "*.sh", "*.yml",
];

/// This file, which names the British spellings it looks for.
const SELF: &str = "apps/rook-cli/src/commands/drift/spelling.rs";

pub(super) fn check(report: &mut Report) -> anyhow::Result<()> {
    for path in tracked(FILES)?.into_iter().filter(|p| p != SELF) {
        let text = fs_err::read_to_string(&path)?;
        for (i, line) in text.lines().enumerate() {
            for word in words(line) {
                if let Some(american) = american(&word) {
                    report.errors.push(format!(
                        "{path}:{}: \"{word}\" is British; write \"{american}\"",
                        i + 1
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The American spelling of `word` (lowercase), if it's British.
pub(super) fn american(word: &str) -> Option<String> {
    for (british, american) in PARTS {
        if word.contains(british) {
            return Some(word.replace(british, american));
        }
    }
    for (british, american) in WORDS {
        if word == *british {
            return Some((*american).to_owned());
        }
    }
    for stem in ISE_STEMS {
        let Some(ending) = word.strip_prefix(stem) else {
            continue;
        };
        if ISE_ENDINGS.contains(&ending) {
            return Some(format!("{stem}{}", ending.replacen('s', "z", 1)));
        }
    }
    None
}

/// The lowercased words of `line`, split at non-letters and at camelCase
/// capitals.
pub(super) fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut previous_lower = false;
    for c in line.chars() {
        let starts_word = c.is_uppercase() && previous_lower;
        if (!c.is_alphabetic() || starts_word) && !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
        if c.is_alphabetic() {
            word.extend(c.to_lowercase());
        }
        previous_lower = c.is_lowercase();
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn british_spellings_are_found() {
        assert_eq!(american("colours").as_deref(), Some("colors"));
        assert_eq!(american("uncentred").as_deref(), Some("uncentered"));
        assert_eq!(american("normalised").as_deref(), Some("normalized"));
        assert_eq!(american("organisation").as_deref(), Some("organization"));
        assert_eq!(american("programme").as_deref(), Some("program"));
        for word in [
            "color",
            "programmer",
            "realistic",
            "emphasis",
            "analysis",
            "promise",
            "noise",
        ] {
            assert_eq!(american(word), None, "{word}");
        }
    }

    #[test]
    fn words_split_at_case_and_symbols() {
        assert_eq!(
            words("let greyLevel = mesh_centre;"),
            ["let", "grey", "level", "mesh", "centre"]
        );
    }
}
