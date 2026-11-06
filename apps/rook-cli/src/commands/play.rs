//! `rook play`: open a place in `rook-player`, which sits in `bin` beside
//! `rook` in a packaged folder and right beside it in a build folder.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;

/// The extensions of the files `rook` opens by itself.
const PLACES: [&str; 2] = ["rbxl", "rbxlx"];

/// What to do without a subcommand: nothing given asks for a place, and a
/// place file opens it, with what follows going to the player. `None`
/// leaves the arguments to the command line.
pub(crate) fn shortcut(args: &[OsString]) -> Option<anyhow::Result<()>> {
    match args.get(1) {
        None => Some(ask()),
        Some(first) if is_place(Path::new(first)) => Some(run(&args[1..])),
        Some(_) => None,
    }
}

fn is_place(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|x| PLACES.iter().any(|p| x.eq_ignore_ascii_case(p)))
        && path.is_file()
}

/// Ask for a place and open it. Cancelling is not a failure.
fn ask() -> anyhow::Result<()> {
    let picked = rfd::FileDialog::new()
        .set_title("Open a place in Rook")
        .add_filter("Roblox places", &PLACES)
        .pick_file();
    match picked {
        Some(place) => run(&[place.into_os_string()]),
        None => Ok(()),
    }
}

/// Run the player with these arguments until it exits, and exit as it did.
pub(crate) fn run(args: &[OsString]) -> anyhow::Result<()> {
    let player = player()?;
    let status = Command::new(&player)
        .args(args)
        .status()
        .with_context(|| format!("couldn't start {}", player.display()))?;
    match status.code() {
        Some(0) => Ok(()),
        Some(code) => std::process::exit(code),
        None => anyhow::bail!("{status}"),
    }
}

fn player() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("couldn't find where rook is")?;
    let dir = exe.parent().context("rook has no folder")?;
    candidates(dir)
        .into_iter()
        .find(|p| p.is_file())
        .with_context(|| format!("no rook-player in {} or its bin folder", dir.display()))
}

fn candidates(dir: &Path) -> [PathBuf; 2] {
    let name = format!("rook-player{}", std::env::consts::EXE_SUFFIX);
    [dir.join("bin").join(&name), dir.join(name)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packaged_player_comes_before_one_beside_rook() {
        let [first, second] = candidates(Path::new("Rook"));
        assert!(first.starts_with("Rook/bin"));
        assert_eq!(second.parent(), Some(Path::new("Rook")));
    }

    #[test]
    fn only_place_files_open_without_a_subcommand() {
        assert!(!is_place(Path::new("dump")));
        assert!(!is_place(Path::new("missing.rbxl")));
        assert!(!is_place(Path::new("Cargo.toml")));
    }

    #[test]
    fn subcommands_are_left_to_the_command_line() {
        let args = ["rook", "dump", "x.rbxl"].map(OsString::from);
        assert!(shortcut(&args).is_none());
    }
}
