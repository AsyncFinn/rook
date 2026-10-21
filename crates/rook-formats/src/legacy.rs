//! Upgrade pass for legacy Roblox XML (roughly 2006–2010): rewrites what
//! `rbx_xml` rejects into modern equivalents, only after a normal load has
//! failed. What each construct becomes, and why, is in
//! `docs/behavior/content/legacy-xml.md`; what was dropped is kept in the report
//! ([`EmbeddedAsset`], [`HashReference`]).

use base64::Engine;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use rbx_dom_weak::WeakDom;
use rbx_types::Variant;

/// Private-use block used to carry forbidden control characters through parsing.
const PUA_BASE: u32 = 0xE000;

/// An asset stored inside a legacy file rather than referenced by ID.
/// Not loaded into the tree; kept for a possible opt-in restore.
#[derive(Debug, Clone)]
pub struct EmbeddedAsset {
    /// The `referent` attribute of the `<Item>` it belonged to in the file.
    pub item_referent: Option<String>,
    /// The Content property it was stored in, as named in the file.
    pub property: Option<String>,
    /// MIME type from the file, if it gave one.
    pub mime_type: Option<String>,
    pub data: Vec<u8>,
}

/// A legacy hash-based asset reference. Not loaded into the tree.
#[derive(Debug, Clone)]
pub struct HashReference {
    pub item_referent: Option<String>,
    pub property: Option<String>,
    pub hash: String,
}

/// What the upgrade pass changed.
#[derive(Debug, Clone, Default)]
pub struct LegacyUpgrade {
    pub externals_dropped: u32,
    pub embedded_assets: Vec<EmbeddedAsset>,
    pub hash_references: Vec<HashReference>,
    /// Forbidden control characters carried through and restored.
    pub control_characters: u32,
    /// True if the file already used the private-use range, so control
    /// characters were replaced with U+FFFD instead of being restored.
    pub control_characters_lossy: bool,
    pub msvc_floats: u32,
    pub null_numbers: u32,
}

impl LegacyUpgrade {
    pub fn changed_anything(&self) -> bool {
        self.externals_dropped > 0
            || !self.embedded_assets.is_empty()
            || !self.hash_references.is_empty()
            || self.control_characters > 0
            || self.msvc_floats > 0
            || self.null_numbers > 0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UpgradeError {
    #[error("legacy XML is not valid UTF-8")]
    NotUtf8,
    #[error("legacy XML could not be read: {0}")]
    Xml(String),
}

/// Rewrite legacy XML into a form `rbx_xml` accepts.
pub fn upgrade(xml: &[u8]) -> Result<(Vec<u8>, LegacyUpgrade), UpgradeError> {
    let text = std::str::from_utf8(xml).map_err(|_| UpgradeError::NotUtf8)?;
    let mut report = LegacyUpgrade {
        control_characters_lossy: text
            .chars()
            .any(|c| (PUA_BASE..PUA_BASE + 0x20).contains(&(c as u32))),
        ..LegacyUpgrade::default()
    };

    let mut reader = Reader::from_str(text);
    let config = reader.config_mut();
    config.trim_text(false);
    config.check_end_names = false;
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut stack: Vec<String> = Vec::new();
    // `referent` of each open <Item>, and the name of the open <Content>.
    let mut items: Vec<Option<String>> = Vec::new();
    let mut content_name: Option<String> = None;
    let mut skip_depth = 0usize;

    loop {
        let event = reader
            .read_event()
            .map_err(|e| UpgradeError::Xml(format!("at byte {}: {e}", reader.buffer_position())))?;
        // Inside an <External> element: drop everything until it closes.
        if skip_depth > 0 {
            match event {
                Event::Start(_) => skip_depth += 1,
                Event::End(_) => skip_depth -= 1,
                Event::Eof => break,
                _ => {}
            }
            continue;
        }
        match event {
            Event::Eof => break,
            Event::Start(start) if start.local_name().as_ref() == "External" => {
                report.externals_dropped += 1;
                skip_depth = 1;
            }
            Event::Empty(start) if start.local_name().as_ref() == "External" => {
                report.externals_dropped += 1;
            }
            Event::Start(start)
                if stack.last().map(String::as_str) == Some("Content")
                    && matches!(start.local_name().as_ref(), "binary" | "hash") =>
            {
                let is_binary = start.local_name().as_ref() == "binary";
                let mime = attribute(&start, "contentType");
                let inner = read_until_end(&mut reader)?;
                let item_referent = items.last().cloned().flatten();
                let property = content_name.clone();
                if is_binary {
                    let cleaned: String = inner.chars().filter(|c| !c.is_whitespace()).collect();
                    let data = base64::engine::general_purpose::STANDARD
                        .decode(cleaned.as_bytes())
                        .unwrap_or_else(|_| inner.clone().into_bytes());
                    report.embedded_assets.push(EmbeddedAsset {
                        item_referent,
                        property,
                        mime_type: mime,
                        data,
                    });
                } else {
                    report.hash_references.push(HashReference {
                        item_referent,
                        property,
                        hash: inner.trim().to_owned(),
                    });
                }
                // Modern Roblox loads these properties empty.
                write(&mut writer, Event::Start(BytesStart::new("null")))?;
                write(&mut writer, Event::End(BytesEnd::new("null")))?;
            }
            Event::Start(start) => {
                let name = start.local_name().as_ref().to_owned();
                match name.as_str() {
                    "Item" => items.push(attribute(&start, "referent")),
                    "Content" => content_name = attribute(&start, "name"),
                    _ => {}
                }
                stack.push(name);
                write(&mut writer, Event::Start(start))?;
            }
            Event::End(end) => {
                match stack.pop().as_deref() {
                    Some("Item") => {
                        items.pop();
                    }
                    Some("Content") => content_name = None,
                    _ => {}
                }
                write(&mut writer, Event::End(end))?;
            }
            Event::Text(t) => {
                let raw: &str = &t; // raw text; rbx_xml does its own line-ending handling
                let numeric = stack.last().is_some_and(|n| is_numeric_element(n));
                let replaced = match (numeric, raw.trim()) {
                    (true, s) if msvc_float(s).is_some() => {
                        report.msvc_floats += 1;
                        Some(msvc_float(s).unwrap_or("NaN").to_owned())
                    }
                    (true, "null") => {
                        report.null_numbers += 1;
                        Some("0".to_owned())
                    }
                    _ => None,
                };
                let content = match replaced {
                    Some(s) => s,
                    None => map_control_chars(raw, &mut report),
                };
                write(&mut writer, Event::Text(BytesText::from_escaped(content)))?;
            }
            Event::GeneralRef(r) => match r.resolve_char_ref() {
                Ok(Some(c)) if is_forbidden_xml_char(c) => {
                    write(
                        &mut writer,
                        Event::Text(BytesText::from_escaped(pua_for(c, &mut report))),
                    )?;
                }
                // `&#0;` fails to resolve at all; it's the most common case.
                Err(_) if matches!(r.as_ref(), "#0" | "#x0" | "#x00") => {
                    write(
                        &mut writer,
                        Event::Text(BytesText::from_escaped(pua_for('\0', &mut report))),
                    )?;
                }
                _ => write(&mut writer, Event::GeneralRef(r))?,
            },
            other => write(&mut writer, other)?,
        }
    }
    Ok((writer.into_inner(), report))
}

/// After loading upgraded XML, turn private-use stand-ins back into the
/// original control characters in names and string properties.
pub fn restore_control_chars(dom: &mut WeakDom, report: &LegacyUpgrade) {
    if report.control_characters == 0 || report.control_characters_lossy {
        return;
    }
    let refs: Vec<_> = dom
        .descendants()
        .map(rbx_dom_weak::Instance::referent)
        .collect();
    for r in refs {
        let Some(inst) = dom.get_by_ref_mut(r) else {
            continue;
        };
        if let Some(fixed) = unmap(&inst.name) {
            inst.name = fixed;
        }
        for value in inst.properties.values_mut() {
            if let Variant::String(s) = value
                && let Some(fixed) = unmap(s)
            {
                *s = fixed;
            }
        }
    }
}

fn unmap(s: &str) -> Option<String> {
    if !s
        .chars()
        .any(|c| (PUA_BASE..PUA_BASE + 0x20).contains(&(c as u32)))
    {
        return None;
    }
    Some(
        s.chars()
            .map(|c| {
                let v = c as u32;
                if (PUA_BASE..PUA_BASE + 0x20).contains(&v) {
                    char::from_u32(v - PUA_BASE).unwrap_or(c)
                } else {
                    c
                }
            })
            .collect(),
    )
}

/// Characters XML 1.0 forbids: C0 controls other than tab, LF and CR.
fn is_forbidden_xml_char(c: char) -> bool {
    (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r')
}

fn pua_for(c: char, report: &mut LegacyUpgrade) -> String {
    report.control_characters += 1;
    if report.control_characters_lossy {
        '\u{FFFD}'.to_string()
    } else {
        char::from_u32(PUA_BASE + c as u32)
            .unwrap_or('\u{FFFD}')
            .to_string()
    }
}

fn map_control_chars(raw: &str, report: &mut LegacyUpgrade) -> String {
    if !raw.chars().any(is_forbidden_xml_char) {
        return raw.to_owned();
    }
    raw.chars()
        .map(|c| {
            if is_forbidden_xml_char(c) {
                pua_for(c, report)
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Elements whose text is a single number in Roblox XML.
fn is_numeric_element(name: &str) -> bool {
    matches!(
        name,
        "float" | "double" | "int" | "int64" | "X" | "Y" | "Z" | "XS" | "XO" | "YS" | "YO"
    ) || (name.len() == 3
        && name.starts_with('R')
        && name[1..].bytes().all(|b| (b'0'..=b'2').contains(&b)))
}

/// MSVC's text for non-finite floats, mapped to forms Rust parses.
fn msvc_float(s: &str) -> Option<&'static str> {
    match s {
        "1.#QNAN" | "-1.#QNAN" | "1.#IND" | "-1.#IND" | "1.#SNAN" | "-1.#SNAN" => Some("NaN"),
        "1.#INF" => Some("inf"),
        "-1.#INF" => Some("-inf"),
        _ => None,
    }
}

fn attribute(start: &BytesStart<'_>, local: &str) -> Option<String> {
    start
        .attributes()
        .flatten()
        .find_map(|a| (a.key.local_name().as_ref() == local).then(|| a.value.to_string()))
}

/// Collect the text of the current element up to its end tag.
fn read_until_end(reader: &mut Reader<&[u8]>) -> Result<String, UpgradeError> {
    let mut out = String::new();
    let mut depth = 1usize;
    loop {
        match reader
            .read_event()
            .map_err(|e| UpgradeError::Xml(e.to_string()))?
        {
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    return Ok(out);
                }
            }
            Event::Text(t) => out.push_str(&t),
            Event::CData(c) => out.push_str(&c),
            Event::GeneralRef(r) => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    out.push(c);
                }
            }
            Event::Eof => return Err(UpgradeError::Xml("unexpected end of file".into())),
            _ => {}
        }
    }
}

fn write(writer: &mut Writer<Vec<u8>>, event: Event<'_>) -> Result<(), UpgradeError> {
    writer
        .write_event(event)
        .map_err(|e| UpgradeError::Xml(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: &str = r#"<roblox xmlns:xmime="http://www.w3.org/2005/05/xmlmime" version="4">
	<External>null</External>
	<External>nil</External>
	<Item class="Part" referent="RBX0">
		<Properties>
			<string name="Name">A&#0;B&#4;C</string>
			<CoordinateFrame name="CFrame">
				<X>1.#QNAN</X><Y>null</Y><Z>-1.#INF</Z>
				<R00>1</R00><R01>0</R01><R02>0</R02>
				<R10>0</R10><R11>1</R11><R12>0</R12>
				<R20>0</R20><R21>0</R21><R22>1</R22>
			</CoordinateFrame>
		</Properties>
		<Item class="Decal" referent="RBX1">
			<Properties>
				<Content name="Texture"><binary xmime:contentType="image/png">aGVs
bG8=</binary></Content>
				<string name="Name">null</string>
			</Properties>
		</Item>
		<Item class="SpecialMesh" referent="RBX2">
			<Properties>
				<Content name="TextureId"><hash>abc123</hash></Content>
			</Properties>
		</Item>
	</Item>
	<External>RBX5</External>
</roblox>"#;

    #[test]
    fn upgrades_and_loads() {
        let loaded = crate::load(LEGACY.as_bytes()).expect("legacy file loads after upgrade");
        let report = loaded.legacy.expect("upgrade ran");
        assert_eq!(report.externals_dropped, 3);
        assert_eq!(report.hash_references.len(), 1);
        assert_eq!(report.hash_references[0].hash, "abc123");
        assert_eq!(
            report.hash_references[0].item_referent.as_deref(),
            Some("RBX2")
        );
        assert_eq!(
            report.hash_references[0].property.as_deref(),
            Some("TextureId")
        );
        assert_eq!(report.control_characters, 2);
        assert_eq!(report.msvc_floats, 2);
        assert_eq!(report.null_numbers, 1);
        assert_eq!(report.embedded_assets.len(), 1);
        assert_eq!(report.embedded_assets[0].data, b"hello");
        assert_eq!(
            report.embedded_assets[0].mime_type.as_deref(),
            Some("image/png")
        );
        assert_eq!(
            report.embedded_assets[0].item_referent.as_deref(),
            Some("RBX1")
        );
        assert_eq!(
            report.embedded_assets[0].property.as_deref(),
            Some("Texture")
        );

        let dom = &loaded.dom;
        let part = dom.descendants().find(|i| i.class == "Part").unwrap();
        assert_eq!(
            part.name, "A\0B\u{4}C",
            "control characters restored exactly"
        );
        let Some(Variant::CFrame(cf)) = part.properties.get(&"CFrame".into()) else {
            panic!("CFrame missing");
        };
        assert!(cf.position.x.is_nan());
        assert_eq!(cf.position.y, 0.0);
        assert_eq!(cf.position.z, f32::NEG_INFINITY);

        // `null` as a *string* is left alone.
        let decal = dom.descendants().find(|i| i.class == "Decal").unwrap();
        assert_eq!(decal.name, "null");

        // Like modern Studio, embedded and hash-referenced content loads empty.
        for inst in dom
            .descendants()
            .filter(|i| matches!(i.class.as_str(), "Decal" | "SpecialMesh"))
        {
            for value in inst.properties.values() {
                let text = format!("{value:?}");
                assert!(!text.contains("abc123") && !text.contains("aGVs"), "{text}");
            }
        }
    }

    #[test]
    fn modern_files_are_untouched() {
        let modern = br#"<roblox version="4"><Item class="Part" referent="RBX0"><Properties><string name="Name">P</string></Properties></Item></roblox>"#;
        assert!(crate::load(modern).unwrap().legacy.is_none());
    }
}
