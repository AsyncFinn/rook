//! Compresses the API data for embedding: the files stay readable in the
//! repository, and the exe carries a fraction of their size.
#![expect(
    clippy::disallowed_methods,
    reason = "a build script runs on the build machine, outside rook-platform"
)]

use std::path::Path;

fn main() {
    let out = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    for name in ["database.msgpack", "members.json"] {
        let source = Path::new("data").join(name);
        println!("cargo::rerun-if-changed={}", source.display());
        let bytes = std::fs::read(&source).expect("API data file exists");
        let packed = zstd::encode_all(bytes.as_slice(), 19).expect("zstd compresses in memory");
        std::fs::write(Path::new(&out).join(format!("{name}.zst")), packed)
            .expect("OUT_DIR is writable");
    }
}
