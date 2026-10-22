//! `rook-tools studio-defaults`: measure in Studio the property defaults
//! the reflection database leaves out, and regenerate Rook's table of them.
//!
//! The database only lists defaults for properties that save to files, so
//! service settings and runtime state would read false, 0 or empty. This
//! lists those gaps, reads each one in Studio (Edit, then a playtest
//! client), and writes the values that differ from Rook's zero value to
//! `crates/rook-dom/src/properties/studio_defaults.rs`. See
//! `docs/behavior/property-defaults.md`.

mod generate;
mod probe;

use std::path::Path;
use std::time::Duration;

use rook_reflection::{ClassId, Database, PropertyId, PropertyKind, Scriptability};

use super::studio::Studio;

/// A class to probe and the properties it has no database default for.
pub(crate) struct Gap {
    pub(crate) class: ClassId,
    /// Read through `GetService` instead of `Instance.new`.
    pub(crate) service: bool,
    pub(crate) properties: Vec<PropertyId>,
}

/// Every creatable class and service, with the scriptable properties the
/// database gives no default for.
pub(crate) fn gaps(db: &Database) -> Vec<Gap> {
    let mut out: Vec<Gap> = db
        .classes()
        .iter()
        .filter(|c| !c.flags.not_creatable || c.flags.service)
        .map(|c| Gap {
            class: c.id,
            service: c.flags.service,
            properties: c
                .all_properties
                .iter()
                .copied()
                .filter(|&p| {
                    let prop = db.property(p);
                    // The DOM keeps these on the instance itself.
                    !matches!(prop.name, "Name" | "Parent" | "ClassName")
                        && matches!(prop.kind, PropertyKind::Canonical(_))
                        && matches!(
                            prop.scriptability,
                            Scriptability::Read | Scriptability::ReadWrite | Scriptability::Custom
                        )
                        && db.default_value(c.id, p).is_none()
                })
                .collect(),
        })
        .filter(|g| !g.properties.is_empty())
        .collect();
    out.sort_by_key(|g| db.class(g.class).name);
    out
}

pub(crate) fn run(list: bool, out: &Path) -> anyhow::Result<()> {
    let db = rook_reflection::database();
    let gaps = gaps(db);
    let count: usize = gaps.iter().map(|g| g.properties.len()).sum();
    if list {
        for gap in &gaps {
            let class = db.class(gap.class).name;
            for &p in &gap.properties {
                println!("{class}.{}", db.property(p).name);
            }
        }
        eprintln!(
            "{count} properties without a default on {} classes",
            gaps.len()
        );
        return Ok(());
    }
    eprintln!("probing {count} properties on {} classes", gaps.len());
    let mut studio = Studio::connect()?;
    anyhow::ensure!(
        studio.mode()? == "Edit",
        "stop the playtest first: the probe reads Edit mode, then starts one"
    );
    let edit = probe::read(&mut studio, db, &gaps, probe::Side::Edit)?;
    studio.set_playing(true)?;
    // Let the client finish joining before reading it.
    std::thread::sleep(Duration::from_secs(5));
    let reads = (|| {
        let client = probe::read(&mut studio, db, &gaps, probe::Side::Client)?;
        // A second read a moment later finds clocks and other values that move.
        std::thread::sleep(Duration::from_secs(2));
        let again = probe::read(&mut studio, db, &gaps, probe::Side::Client)?;
        anyhow::Ok((client, again))
    })();
    let stopped = studio.set_playing(false);
    let (client, again) = reads?;
    stopped?;
    generate::write(db, &gaps, &edit, &client, &again, out)
}
