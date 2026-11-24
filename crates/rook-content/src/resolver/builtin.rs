//! Rook's built-in images, `rbxasset://rook/<name>`: drawn from
//! `resources/` when Rook is built (`build.rs`) and kept in the binary, so
//! Rook's own GUI (its CoreScripts) has its logo and icons with no files
//! beside it. Roblox's content has no `rook` folder.

/// Each image's path below `rook/` and its PNG.
static IMAGES: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/builtin.rs"));

/// The built-in file an `rbxasset://` path names, compared without case.
pub fn find(path: &str) -> Option<&'static [u8]> {
    let name = path.trim_start_matches('/');
    let name = name
        .get(..5)
        .filter(|p| p.eq_ignore_ascii_case("rook/"))
        .map(|_| &name[5..])?;
    IMAGES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|&(_, bytes)| bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_and_icons_are_built_in() {
        for path in ["rook/logo.png", "rook/logo-mark.png", "Rook/Icons/Settings.png"] {
            let png = find(path).unwrap_or_else(|| panic!("{path}"));
            assert_eq!(&png[1..4], b"PNG", "{path}");
        }
        assert!(find("rook/none.png").is_none());
        assert!(find("textures/rook/logo.png").is_none());
    }
}
