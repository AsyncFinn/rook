//! Compiled scripts in binary files.
//!
//! Roblox's own packages (Studio's `DataModelPatch.rbxm`, which holds the
//! CoreScripts and CorePackages) save each script's Luau bytecode in
//! `Source` under property type 0x1D, which `rbx_binary` skips as unknown.
//! This reads just those values, keyed by where each script sits, so a
//! caller can match them to the tree `load` builds.

use crate::LoadError;
use crate::chunks::{Reader, chunks};

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
}
