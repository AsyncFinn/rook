//! Binary files that store a property as a type the reflection database
//! no longer gives it. `rbx_binary` rejects the whole file then; Studio
//! converts. Text objects' `TextSize` was saved as an Int32 in files from
//! about 2016 and is a Float32 now. This rewrites such properties' chunks
//! as the database's type, so the file can be decoded again.

use rbx_reflection::{DataType, ReflectionDatabase};
use rbx_types::VariantType;

use crate::LoadError;
use crate::chunks::{HEADER, Reader, chunks, interleave, interleaved, unzigzag, write_chunk};

/// The binary file's property type ids for what can be converted.
const INT32: u8 = 0x03;
const FLOAT32: u8 = 0x04;

/// `bytes` with each property stored as a type it converts from rewritten
/// as the database's type, or `None` when there's none.
pub(crate) fn retype(bytes: &[u8], db: &ReflectionDatabase) -> Result<Option<Vec<u8>>, LoadError> {
    let mut out = bytes[..HEADER].to_vec();
    let mut classes: Vec<(u32, String)> = Vec::new();
    let mut changed = false;
    for chunk in chunks(bytes)? {
        if !matches!(chunk.name, b"INST" | b"PROP") {
            out.extend_from_slice(chunk.stored);
            continue;
        }
        let data = chunk.data()?;
        let mut r = Reader::new(&data);
        let class = r.u32()?;
        let name = String::from_utf8_lossy(r.string()?).into_owned();
        if chunk.name == b"INST" {
            classes.push((class, name));
            out.extend_from_slice(chunk.stored);
            continue;
        }
        let stored = r.u8()?;
        let class_name = classes.iter().find(|(c, _)| *c == class).map(|(_, n)| n);
        let wanted = class_name.and_then(|c| canonical_type(db, c, &name));
        let values = r.rest();
        match (stored, wanted) {
            (INT32, Some(VariantType::Float32)) => {
                let floats: Vec<u32> = interleaved(values)
                    .map(|v| (unzigzag(v) as f32).to_bits().rotate_left(1))
                    .collect();
                let mut rewritten = data[..r.at - values.len() - 1].to_vec();
                rewritten.push(FLOAT32);
                rewritten.extend_from_slice(&interleave(&floats));
                write_chunk(&mut out, b"PROP", &rewritten);
                changed = true;
            }
            _ => out.extend_from_slice(chunk.stored),
        }
    }
    Ok(changed.then_some(out))
}

/// The type `class` (or a class it inherits from) gives `property`.
fn canonical_type(db: &ReflectionDatabase, class: &str, property: &str) -> Option<VariantType> {
    let descriptor = db.classes.get(class)?;
    db.superclasses_iter(descriptor)
        .find_map(|c| c.properties.get(property))
        .map(|p| match p.data_type {
            DataType::Value(ty) => ty,
            _ => VariantType::Enum,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::write_chunk;

    /// A file holding one TextLabel whose `TextSize` is stored as the
    /// Int32 `size`.
    fn file_with_int_text_size(size: i32) -> Vec<u8> {
        let mut out = b"<roblox!\x89\xff\x0d\x0a\x1a\x0a\0\0".to_vec();
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        let string = |out: &mut Vec<u8>, s: &[u8]| {
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s);
        };
        let mut inst = 0u32.to_le_bytes().to_vec();
        string(&mut inst, b"TextLabel");
        inst.push(0);
        inst.extend_from_slice(&1u32.to_le_bytes());
        inst.extend_from_slice(&interleave(&[0]));
        write_chunk(&mut out, b"INST", &inst);
        let mut prop = 0u32.to_le_bytes().to_vec();
        string(&mut prop, b"TextSize");
        prop.push(INT32);
        let zigzag = ((size << 1) ^ (size >> 31)) as u32;
        prop.extend_from_slice(&interleave(&[zigzag]));
        write_chunk(&mut out, b"PROP", &prop);
        let mut prnt = vec![0];
        prnt.extend_from_slice(&1u32.to_le_bytes());
        prnt.extend_from_slice(&interleave(&[0]));
        prnt.extend_from_slice(&interleave(&[1]));
        write_chunk(&mut out, b"PRNT", &prnt);
        write_chunk(&mut out, b"END\0", b"</roblox>");
        out
    }

    #[test]
    fn int_text_size_loads_as_float() {
        let loaded = crate::load(&file_with_int_text_size(24)).unwrap();
        let label = loaded.dom.root().children()[0];
        let label = loaded.dom.get_by_ref(label).unwrap();
        assert_eq!(
            label.properties.get(&"TextSize".into()),
            Some(&rbx_types::Variant::Float32(24.0))
        );
    }
}
