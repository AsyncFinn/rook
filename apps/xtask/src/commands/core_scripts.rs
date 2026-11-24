//! `cargo xtask core-scripts`: build Rook's CoreScripts
//! (`crates/rook-engine/corescripts`) into the `.rbxm` the engine loads
//! beside its exe (`rook_engine::core_scripts::rook`), its scripts
//! compiled to bytecode.

use std::path::Path;

use rook_engine::core_scripts::rook;

pub(crate) fn run(out: &Path) -> anyhow::Result<()> {
    let bytes = rook::build(Path::new(rook::SOURCE)).map_err(anyhow::Error::msg)?;
    if let Some(dir) = out.parent() {
        fs_err::create_dir_all(dir)?;
    }
    fs_err::write(out, &bytes)?;
    println!("wrote {} ({} KB)", out.display(), bytes.len() / 1024);
    Ok(())
}
