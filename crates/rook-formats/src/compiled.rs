//! Compiled scripts in binary files.
//!
//! Roblox's own packages (Studio's `DataModelPatch.rbxm`, which holds the
//! CoreScripts and CorePackages) save each script's Luau bytecode in
//! `Source` under property type 0x1D, which `rbx_binary` skips as unknown.
//! This reads just those values, keyed by where each script sits, so a
//! caller can match them to the tree `load` builds, and writes them
//! (`compile_scripts`) for packages built the same way, Rook's own
//! CoreScripts.

use crate::LoadError;
use crate::chunks::{HEADER, Reader, chunks, write_chunk};
use rustc_hash::FxHashMap;

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

/// The property type of a string, which `rbx_binary` saves `Source` as.
const STRING: u8 = 0x01;

/// Every compiled script in a binary file. Files without any (all places
/// and models people make) give an empty list.
pub fn compiled_scripts(bytes: &[u8]) -> Result<Vec<CompiledScript>, LoadError> {
    // Referents by class, the bytecode by referent, and each referent's
    // parent in file order (the order `rbx_binary` adds children in).
    let mut referents: Vec<(u32, Vec<i32>)> = Vec::new();
    let mut code: Vec<(i32, Vec<u8>)> = Vec::new();
    let mut parents: Vec<(i32, i32)> = Vec::new();
    for chunk in chunks(bytes)? {
        if !matches!(chunk.name, b"INST" | b"PROP" | b"PRNT") {
            continue;
        }
        let data = chunk.data()?;
        let mut r = Reader::new(&data);
        match chunk.name {
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
                    return Err(LoadError::Binary(
                        "compiled scripts: PROP before its INST".into(),
                    ));
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
    let mut place: FxHashMap<i32, (i32, u32)> = Default::default();
    let mut counts: FxHashMap<i32, u32> = Default::default();
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

/// `bytes`, a binary file, with every script's `Source` saved as its
/// bytecode under [`BYTECODE`], as Roblox's packages save theirs, so
/// [`compiled_scripts`] reads it back. `compile` turns a script's source
/// into its bytecode. A file with no scripts comes back as it was.
pub fn compile_scripts(
    bytes: &[u8],
    mut compile: impl FnMut(&[u8]) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let mut out = bytes.get(..HEADER).ok_or("not a binary file")?.to_vec();
    for chunk in chunks(bytes).map_err(|e| e.to_string())? {
        if chunk.name != b"PROP" {
            out.extend_from_slice(chunk.stored);
            continue;
        }
        let data = chunk.data().map_err(|e| e.to_string())?;
        let mut r = Reader::new(&data);
        let read = |r: &mut Reader<'_>| -> Result<bool, LoadError> {
            r.u32()?;
            Ok(r.string()? == b"Source" && r.u8()? == STRING)
        };
        if !read(&mut r).map_err(|e| e.to_string())? {
            out.extend_from_slice(chunk.stored);
            continue;
        }
        let mut rewritten = data[..r.at - 1].to_vec();
        rewritten.push(BYTECODE);
        while r.at < data.len() {
            let source = r.string().map_err(|e| e.to_string())?;
            let code = compile(source)?;
            let len = u32::try_from(code.len()).map_err(|e| e.to_string())?;
            rewritten.extend_from_slice(&len.to_le_bytes());
            rewritten.extend_from_slice(&code);
        }
        write_chunk(&mut out, b"PROP", &rewritten);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_without_bytecode_give_nothing() {
        let dom = rbx_dom_weak::WeakDom::new(
            rbx_dom_weak::InstanceBuilder::new("DataModel")
                .with_child(rbx_dom_weak::InstanceBuilder::new("Folder")),
        );
        let bytes = crate::save(&dom, crate::Format::Binary).unwrap();
        assert!(compiled_scripts(&bytes).unwrap().is_empty());
    }

    /// Scripts saved as bytecode read back where they sit, and the file
    /// still loads, as Roblox's packages do.
    #[test]
    fn compiled_scripts_read_back() {
        use rbx_dom_weak::{InstanceBuilder, WeakDom};
        let module = |name: &str, source: &str| {
            InstanceBuilder::new("ModuleScript")
                .with_name(name)
                .with_property("Source", source)
        };
        let dom = WeakDom::new(
            InstanceBuilder::new("DataModel").with_child(
                InstanceBuilder::new("Folder")
                    .with_child(module("A", "return 1"))
                    .with_child(InstanceBuilder::new("Folder").with_child(module("B", "return 2"))),
            ),
        );
        let bytes = crate::save(&dom, crate::Format::Binary).unwrap();
        let compiled = compile_scripts(&bytes, |source| Ok(source.to_ascii_uppercase())).unwrap();
        let mut scripts = compiled_scripts(&compiled).unwrap();
        scripts.sort_by(|a, b| a.path.cmp(&b.path));
        let found: Vec<_> = scripts
            .iter()
            .map(|s| (s.path.clone(), s.bytecode.as_slice()))
            .collect();
        assert_eq!(
            found,
            [
                (vec![0, 0], &b"RETURN 1"[..]),
                (vec![0, 1, 0], &b"RETURN 2"[..]),
            ]
        );
        let loaded = crate::load(&compiled).unwrap().dom;
        assert_eq!(loaded.root().children().len(), 1);
    }
}
