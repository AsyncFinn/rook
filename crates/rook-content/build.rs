//! Rook's built-in images, drawn from `resources/` at build time and
//! served as `rbxasset://rook/<name>.png` (`resolver::builtin`): its logo
//! from `rook.svg`, and the white rook alone from the same file
//! (`logo-mark`). See resources/README.md.
#![expect(
    clippy::disallowed_methods,
    reason = "a build script runs on the build machine, outside rook-platform"
)]

use std::{env, fmt::Write, fs, path::PathBuf};

use resvg::{tiny_skia, usvg};

/// The side every image is drawn at: GUI shrinks it to the size it's shown.
const SIDE: u32 = 128;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let resources = manifest.join("../../resources");
    let out = PathBuf::from(env::var("OUT_DIR").expect("set by cargo"));
    let mut images: Vec<(String, String)> = Vec::new();

    let logo_path = resources.join("rook.svg");
    println!("cargo::rerun-if-changed={}", logo_path.display());
    let logo = fs::read_to_string(&logo_path).expect("read rook.svg");
    images.push(("logo.png".into(), logo.clone()));
    // The mark: the icon without its tile, the one <rect> in it.
    let start = logo.find("<rect").expect("rook.svg's tile");
    let end = start + logo[start..].find("/>").expect("the tile's end") + 2;
    images.push((
        "logo-mark.png".into(),
        format!("{}{}", &logo[..start], &logo[end..]),
    ));

    let mut table = String::from("&[\n");
    for (i, (name, svg)) in images.iter().enumerate() {
        let tree = usvg::Tree::from_data(svg.as_bytes(), &usvg::Options::default())
            .unwrap_or_else(|e| panic!("parse {name}: {e}"));
        let file = out.join(format!("builtin{i}.png"));
        fs::write(&file, png(&tree)).expect("write a built-in image");
        let file = file.display().to_string().replace('\\', "/");
        writeln!(table, "    ({name:?}, include_bytes!({file:?})),").expect("a String");
    }
    table.push(']');
    fs::write(out.join("builtin.rs"), table).expect("write builtin.rs");
}

fn png(tree: &usvg::Tree) -> Vec<u8> {
    let mut pixmap = tiny_skia::Pixmap::new(SIDE, SIDE).expect("nonzero size");
    let scale = SIDE as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().expect("encode PNG")
}
