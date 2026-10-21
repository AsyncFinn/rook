//! Loading and saving Roblox place and model files.
//!
//! Wraps `rbx_binary` and `rbx_xml`, pinned to Rook's reflection database.
//! Both decoders map legacy serialized names to canonical properties (for
//! example `size` → `Size`) and run the database's migrations, the same way
//! Studio upgrades old files on load. Properties neither decoder recognizes
//! are kept, so saving a file never silently drops data.
//!
//! Legacy XML (roughly 2006–2010) that `rbx_xml` rejects is retried through
//! the [`legacy`] upgrade pass. Files that load normally are never rewritten.
//!
//! Decoders are run behind a panic guard: a corrupt file produces a
//! [`LoadError`], never a crash.

mod compare;
mod convert;
pub mod legacy;

pub use compare::{CompareOptions, compare, compare_with};
pub use convert::{from_dom, into_dom};

use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};

use rbx_dom_weak::WeakDom;

/// On-disk encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `.rbxl` / `.rbxm`
    Binary,
    /// `.rbxlx` / `.rbxmx`
    Xml,
}

impl Format {
    /// Detect the encoding from the file's first bytes.
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        let start = trim_start(bytes);
        if start.starts_with(b"<roblox!") {
            Some(Self::Binary)
        } else if start.starts_with(b"<roblox") || start.starts_with(b"<?xml") {
            Some(Self::Xml)
        } else {
            None
        }
    }
}

/// Skip a UTF-8 byte-order mark and leading whitespace (seen in hand-edited XML files).
fn trim_start(bytes: &[u8]) -> &[u8] {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    &bytes[start..]
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("not a Roblox place or model file")]
    UnknownFormat,
    #[error("binary decode failed: {0}")]
    Binary(String),
    #[error("XML decode failed: {0}")]
    Xml(String),
    #[error("decoder panicked: {0}")]
    Panicked(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("binary encode failed: {0}")]
    Binary(String),
    #[error("XML encode failed: {0}")]
    Xml(String),
    #[error("encoder panicked: {0}")]
    Panicked(String),
}

/// A loaded file.
#[derive(Debug)]
pub struct Loaded {
    pub format: Format,
    /// The decoded tree. Its root is a placeholder; the file's top-level
    /// instances are the root's children.
    pub dom: WeakDom,
    /// Set when the file needed the legacy XML upgrade pass to load.
    pub legacy: Option<legacy::LegacyUpgrade>,
}

/// Load a place or model file from memory.
pub fn load(bytes: &[u8]) -> Result<Loaded, LoadError> {
    let format = Format::detect(bytes).ok_or(LoadError::UnknownFormat)?;
    match decode(bytes, format) {
        Ok(dom) => Ok(Loaded {
            format,
            dom,
            legacy: None,
        }),
        Err(LoadError::Xml(original)) => load_legacy_xml(bytes).map_err(|retry| {
            // Report the original failure; mention the retry only if it got further.
            match retry {
                Some(e) => LoadError::Xml(format!("{original} (legacy upgrade also failed: {e})")),
                None => LoadError::Xml(original),
            }
        }),
        Err(e) => Err(e),
    }
}

/// Retry an XML file through the legacy upgrade. `Err(None)` means the file
/// had nothing to upgrade, so the original error stands.
fn load_legacy_xml(bytes: &[u8]) -> Result<Loaded, Option<String>> {
    let (upgraded, report) = guard(|| legacy::upgrade(bytes))
        .map_err(Some)?
        .map_err(|e| Some(e.to_string()))?;
    if !report.changed_anything() {
        return Err(None);
    }
    let mut dom = decode(&upgraded, Format::Xml).map_err(|e| Some(e.to_string()))?;
    legacy::restore_control_chars(&mut dom, &report);
    Ok(Loaded {
        format: Format::Xml,
        dom,
        legacy: Some(report),
    })
}

fn decode(bytes: &[u8], format: Format) -> Result<WeakDom, LoadError> {
    let db = rook_reflection::rbx_database();
    guard(|| match format {
        Format::Binary => rbx_binary::Deserializer::new()
            .reflection_database(db)
            .deserialize(Cursor::new(bytes))
            .map_err(|e| LoadError::Binary(e.to_string())),
        Format::Xml => {
            let options = rbx_xml::DecodeOptions::new()
                .reflection_database(db)
                .property_behavior(rbx_xml::DecodePropertyBehavior::ReadUnknown);
            rbx_xml::from_reader(Cursor::new(bytes), options)
                .map_err(|e| LoadError::Xml(e.to_string()))
        }
    })
    .map_err(LoadError::Panicked)?
}

/// Save the root's children of `dom` in the given format.
pub fn save(dom: &WeakDom, format: Format) -> Result<Vec<u8>, SaveError> {
    let db = rook_reflection::rbx_database();
    let refs = dom.root().children().to_vec();
    guard(|| {
        let mut out = Vec::new();
        match format {
            Format::Binary => rbx_binary::Serializer::new()
                .reflection_database(db)
                .serialize(&mut out, dom, &refs)
                .map_err(|e| SaveError::Binary(e.to_string()))?,
            Format::Xml => {
                let options = rbx_xml::EncodeOptions::new()
                    .reflection_database(db)
                    .property_behavior(rbx_xml::EncodePropertyBehavior::WriteUnknown);
                rbx_xml::to_writer(&mut out, dom, &refs, options)
                    .map_err(|e| SaveError::Xml(e.to_string()))?;
            }
        }
        Ok(out)
    })
    .map_err(SaveError::Panicked)?
}

/// Run `f`, converting a panic into its message.
fn guard<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|payload| {
        payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbx_dom_weak::InstanceBuilder;
    use rbx_types::{Variant, Vector3};

    fn sample_dom() -> WeakDom {
        let part = InstanceBuilder::new("Part")
            .with_name("Brick")
            .with_property("Size", Vector3::new(4.0, 1.2, 2.0))
            .with_property("Anchored", true);
        let model = InstanceBuilder::new("Model")
            .with_name("Thing")
            .with_child(part);
        WeakDom::new(InstanceBuilder::new("DataModel").with_child(model))
    }

    #[test]
    fn detects_formats() {
        assert_eq!(Format::detect(b"<roblox!\x89\xff"), Some(Format::Binary));
        assert_eq!(Format::detect(b"<roblox version=\"4\">"), Some(Format::Xml));
        assert_eq!(Format::detect(b"\xEF\xBB\xBF  <roblox>"), Some(Format::Xml));
        assert_eq!(Format::detect(b"PK\x03\x04"), None);
    }

    #[test]
    fn round_trips_both_formats() {
        let dom = sample_dom();
        for format in [Format::Binary, Format::Xml] {
            let bytes = save(&dom, format).unwrap();
            let loaded = load(&bytes).unwrap();
            assert_eq!(loaded.format, format);
            let diffs = compare(&dom, &loaded.dom);
            assert!(diffs.is_empty(), "{format:?}: {diffs:#?}");
            let part = loaded
                .dom
                .descendants()
                .find(|i| i.class == "Part")
                .expect("part survives");
            assert_eq!(
                part.properties.get(&"Anchored".into()),
                Some(&Variant::Bool(true))
            );
        }
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(matches!(load(b"hello"), Err(LoadError::UnknownFormat)));
        let mut truncated = save(&sample_dom(), Format::Binary).unwrap();
        truncated.truncate(40);
        assert!(load(&truncated).is_err());
    }
}
