//! Compiled scripts in binary files.
//!
//! Roblox's own packages (Studio's `DataModelPatch.rbxm`, which holds the
//! CoreScripts and CorePackages) save each script's Luau bytecode in
//! `Source` under property type 0x1D, which `rbx_binary` skips as unknown.
//! This reads just those values, keyed by where each script sits, so a
//! caller can match them to the tree `load` builds.

use crate::LoadError;

/// One compiled script: the child indices from the file's top level down
/// to it (`[2, 0]` is the first child of the third top-level instance),
/// in the order `load` gives children, and its bytecode.
#[derive(Debug)]
pub struct CompiledScript {
    pub path: Vec<u32>,
    pub bytecode: Vec<u8>,
}

/// The property type Roblox saves script bytecode under.
const BYTECODE: u8 = 0x1D;

/// Every compiled script in a binary file. Files without any (all places
/// and models people make) give an empty list.
pub fn compiled_scripts(bytes: &[u8]) -> Result<Vec<CompiledScript>, LoadError> {
    let bad = |what: &str| LoadError::Binary(format!("compiled scripts: {what}"));
    if !bytes.starts_with(b"<roblox!") || bytes.len() < 32 {
        return Err(LoadError::UnknownFormat);
    }
    // Referents by class, the bytecode by referent, and each referent's
    // parent in file order (the order `rbx_binary` adds children in).
    let mut referents: Vec<(u32, Vec<i32>)> = Vec::new();
    let mut code: Vec<(i32, Vec<u8>)> = Vec::new();
    let mut parents: Vec<(i32, i32)> = Vec::new();
    let mut pos = 32;
    while pos + 16 <= bytes.len() {
        let name = &bytes[pos..pos + 4];
        let compressed = read_u32(bytes, pos + 4) as usize;
        let size = read_u32(bytes, pos + 8) as usize;
        pos += 16;
        let stored = if compressed == 0 { size } else { compressed };
        let raw = bytes
            .get(pos..pos + stored)
            .ok_or_else(|| bad("truncated chunk"))?;
        pos += stored;
        if name == b"END\0" {
            break;
        }
        if !matches!(name, b"INST" | b"PROP" | b"PRNT") {
            continue;
        }
        let chunk = decompress(raw, compressed, size).ok_or_else(|| bad("bad compression"))?;
        let mut r = Reader {
            bytes: &chunk,
            at: 0,
        };
        match name {
            b"INST" => {
                let class = r.u32()?;
                let _name = r.string()?;
                let _format = r.u8()?;
                let count = r.u32()? as usize;
                referents.push((class, r.referents(count)?));
            }
            b"PROP" => {
                let class = r.u32()?;
                let property = r.string()?;
                if property != b"Source" || r.u8()? != BYTECODE {
                    continue;
                }
                let Some((_, refs)) = referents.iter().find(|(c, _)| *c == class) else {
                    return Err(bad("PROP before its INST"));
                };
                for &referent in refs {
                    code.push((referent, r.string()?.to_vec()));
                }
            }
            _ => {
                let _version = r.u8()?;
                let count = r.u32()? as usize;
                let children = r.referents(count)?;
                let of = r.referents(count)?;
                parents.extend(children.into_iter().zip(of));
            }
        }
    }
    if code.is_empty() {
        return Ok(Vec::new());
    }
    // Each instance's index among its parent's children (top-level ones
    // count among each other), then the paths down to the scripts.
    let mut place: std::collections::HashMap<i32, (i32, u32)> = Default::default();
    let mut counts: std::collections::HashMap<i32, u32> = Default::default();
    for &(child, parent) in &parents {
        let n = counts.entry(parent).or_default();
        place.insert(child, (parent, *n));
        *n += 1;
    }
    let mut out = Vec::with_capacity(code.len());
    for (referent, bytecode) in code {
        let mut path = Vec::new();
        let mut at = referent;
        while let Some(&(parent, index)) = place.get(&at) {
            path.push(index);
            if parent < 0 {
                break;
            }
            at = parent;
        }
        path.reverse();
        out.push(CompiledScript { path, bytecode });
    }
    Ok(out)
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    bytes
        .get(at..at + 4)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn decompress(raw: &[u8], compressed: usize, size: usize) -> Option<Vec<u8>> {
    if compressed == 0 {
        return Some(raw.to_vec());
    }
    if raw.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        zstd::bulk::decompress(raw, size).ok()
    } else {
        lz4_flex::block::decompress(raw, size).ok()
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], LoadError> {
        let out = self
            .bytes
            .get(self.at..self.at + n)
            .ok_or_else(|| LoadError::Binary("compiled scripts: truncated chunk".into()))?;
        self.at += n;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, LoadError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, LoadError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Result<&[u8], LoadError> {
        let n = self.u32()? as usize;
        self.take(n)
    }

    /// `count` referents: big-endian i32s split into four byte planes,
    /// zigzag-encoded, each the difference from the one before.
    fn referents(&mut self, count: usize) -> Result<Vec<i32>, LoadError> {
        let planes = self.take(count * 4)?;
        let mut out = Vec::with_capacity(count);
        let mut last = 0i32;
        for i in 0..count {
            let v = u32::from_be_bytes([
                planes[i],
                planes[count + i],
                planes[2 * count + i],
                planes[3 * count + i],
            ]);
            let delta = ((v >> 1) as i32) ^ -((v & 1) as i32);
            last = last.wrapping_add(delta);
            out.push(last);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referents_undo_zigzag_and_deltas() {
        // 1, then +2, then -1: zigzag 2, 4, 1, as four byte planes.
        let planes = [0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 4, 1];
        let mut r = Reader {
            bytes: &planes,
            at: 0,
        };
        assert_eq!(r.referents(3).unwrap(), vec![1, 3, 2]);
    }

    #[test]
    fn files_without_bytecode_give_nothing() {
        let dom = rbx_dom_weak::WeakDom::new(
            rbx_dom_weak::InstanceBuilder::new("DataModel")
                .with_child(rbx_dom_weak::InstanceBuilder::new("Folder")),
        );
        let bytes = crate::save(&dom, crate::Format::Binary).unwrap();
        assert!(compiled_scripts(&bytes).unwrap().is_empty());
    }
}
