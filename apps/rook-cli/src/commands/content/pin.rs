//! `rook-tools content --pin`: write the kept content list
//! (`crates/rook-content/data/kept.txt`) again, from the materials' and
//! terrain's tables and each asset's version on asset delivery today.

use std::collections::BTreeSet;
use std::path::Path;

use rook_content::materials::TexturePack;
use rook_content::resolver::delivery::Delivery;
use rook_platform::endpoints::Endpoints;

/// Every pack Rook loads itself: part materials' and terrain's.
fn packs() -> BTreeSet<u64> {
    rook_content::materials::all_packs()
        .chain(rook_terrain::all_packs())
        .collect()
}

pub(crate) fn pin(out: &Path) -> anyhow::Result<()> {
    // Versions are Roblox's, whatever endpoints the settings choose.
    let delivery = Delivery::new(None, &Endpoints::default())
        .expect("asset delivery has an endpoint by default");
    let mut lines = vec![
        "# Content Rook keeps for good: built-in materials' and terrain's".to_owned(),
        "# texture packs and the images they name, each pinned to a version".to_owned(),
        "# (rook_content::resolver::kept). Written by `rook-tools content --pin`.".to_owned(),
        "# <asset id> <version>  # what".to_owned(),
    ];
    // Packs share images (one image as both metalness and roughness).
    let mut seen = BTreeSet::new();
    for pack in packs() {
        let asset = delivery
            .fetch(pack, None)
            .map_err(|e| anyhow::anyhow!("pack {pack}: {e}"))?;
        let version = asset
            .version
            .ok_or_else(|| anyhow::anyhow!("pack {pack}: no version"))?;
        seen.insert(pack);
        lines.push(format!("{pack} {version}  # texture pack"));
        let parsed = TexturePack::parse(&asset.bytes)
            .ok_or_else(|| anyhow::anyhow!("{pack} isn't a TexturePack"))?;
        let maps = [
            ("color", parsed.color),
            ("normal", parsed.normal),
            ("metalness", parsed.metalness),
            ("roughness", parsed.roughness),
        ];
        for (name, image) in maps {
            let Some(image) = image.filter(|&i| seen.insert(i)) else {
                continue;
            };
            let version = delivery
                .latest_version(image)
                .map_err(|e| anyhow::anyhow!("image {image}: {e}"))?;
            lines.push(format!("{image} {version}  # {pack} {name}"));
        }
    }
    fs_err::write(out, lines.join("\n") + "\n")?;
    println!("wrote {} assets to {}", lines.len() - 4, out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_pack_is_kept() {
        for pack in super::packs() {
            assert!(
                rook_content::resolver::kept::version(pack).is_some(),
                "pack {pack} isn't kept: run `rook-tools content --pin`"
            );
        }
    }
}
