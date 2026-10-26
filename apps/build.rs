//! Both apps' build script: renders `resources/rook.svg` into a Windows icon and
//! embeds it in the executable as icon resource 1, which the player's window
//! also loads, and `<app>/<app>.manifest` as its manifest where there is
//! one. See resources/README.md.

use std::{env, fs, path::PathBuf};

use resvg::{tiny_skia, usvg};

/// Every size Windows draws the icon at, each rendered from the SVG so none
/// is shrunk from another: the title bar's (16 pixels at 100%) and the
/// taskbar's (32) at 100-300% display scaling, and 256 for large views.
const SIZES: [u32; 14] = [16, 20, 24, 28, 32, 36, 40, 48, 56, 64, 72, 80, 96, 256];

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let svg = manifest.join("../../resources/rook.svg");
    println!("cargo::rerun-if-changed={}", svg.display());
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let data = fs::read(&svg).expect("read rook.svg");
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).expect("parse rook.svg");
    let out = PathBuf::from(env::var("OUT_DIR").expect("set by cargo"));
    let ico = out.join("rook.ico");
    fs::write(&ico, ico_file(&tree)).expect("write rook.ico");
    let rc = out.join("resources.rc");
    let mut resources = format!("1 ICON \"{}\"\n", rc_path(&ico));
    // An app's own manifest, as resource 1 of type 24 (RT_MANIFEST).
    let app_manifest = manifest.join(format!(
        "{}.manifest",
        env::var("CARGO_PKG_NAME").expect("set by cargo")
    ));
    if app_manifest.exists() {
        println!("cargo::rerun-if-changed={}", app_manifest.display());
        resources += &format!("1 24 \"{}\"\n", rc_path(&app_manifest));
    }
    fs::write(&rc, resources).expect("write resources.rc");
    embed_resource::compile(&rc, embed_resource::NONE)
        .manifest_required()
        .expect("compile resources.rc");
}

/// A path as a resource script quotes it.
fn rc_path(path: &std::path::Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// An .ico holding a PNG of the icon at each of [`SIZES`].
fn ico_file(tree: &usvg::Tree) -> Vec<u8> {
    let images: Vec<Vec<u8>> = SIZES.iter().map(|&side| png(tree, side)).collect();
    let mut ico = vec![0, 0, 1, 0];
    ico.extend((SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * SIZES.len();
    for (&side, image) in SIZES.iter().zip(&images) {
        // A side of 0 means 256.
        let side = if side >= 256 { 0 } else { side as u8 };
        ico.extend([side, side, 0, 0]);
        ico.extend(1u16.to_le_bytes());
        ico.extend(32u16.to_le_bytes());
        ico.extend((image.len() as u32).to_le_bytes());
        ico.extend((offset as u32).to_le_bytes());
        offset += image.len();
    }
    for image in images {
        ico.extend(image);
    }
    ico
}

fn png(tree: &usvg::Tree, side: u32) -> Vec<u8> {
    let mut pixmap = tiny_skia::Pixmap::new(side, side).expect("nonzero size");
    let scale = side as f32 / tree.size().width();
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().expect("encode icon PNG")
}
