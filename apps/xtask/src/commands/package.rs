//! `cargo xtask package`: the folder to hand to other people, built with
//! `cargo dist` and holding nothing else. This is the one list of what a
//! packaged Rook is; `rook play` finds the player by the same layout.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use fs_err as fs;

/// Rook's own files beside the programs, from the repository's root.
const FILES: [&str; 2] = ["LICENSE", "NOTICE"];

const README: &str = include_str!("package/README.txt");

pub(crate) fn run() -> anyhow::Result<()> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .arg("dist")
        .status()
        .context("couldn't run cargo dist")?;
    anyhow::ensure!(status.success(), "cargo dist failed: {status}");

    let target = target_dir();
    let folder = target
        .join("package")
        .join(folder_name(env!("CARGO_PKG_VERSION")));
    if folder.exists() {
        fs::remove_dir_all(&folder)?;
    }
    fs::create_dir_all(folder.join("bin"))?;
    let built = target.join("dist");
    let exe = std::env::consts::EXE_SUFFIX;
    fs::copy(
        built.join(format!("rook{exe}")),
        folder.join(format!("rook{exe}")),
    )?;
    fs::copy(
        built.join(format!("rook-player{exe}")),
        folder.join("bin").join(format!("rook-player{exe}")),
    )?;
    for file in FILES {
        fs::copy(file, folder.join(file))?;
    }
    fs::write(folder.join("README.txt"), README)?;
    println!("{}", folder.display());
    Ok(())
}

fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| Path::new("target").into(), Into::into)
}

fn folder_name(version: &str) -> String {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    format!("Rook-{version}-{os}-{arch}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_is_named_for_its_version() {
        assert!(folder_name("1.2.3").starts_with("Rook-1.2.3-"));
    }
}
