//! `cargo xtask content <reference>...`: what each reference means and where its
//! bytes come from.

use super::*;

fn print_fetched(f: &Fetched) {
    println!("  from:     {}", f.source.kind.name());
    println!("  location: {}", f.source.location);
    println!(
        "  size:     {} bytes ({})",
        f.bytes.len(),
        describe(&f.bytes)
    );
    println!("  blake3:   {}", f.hash.to_hex());
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "a command like the others, which main runs the same way"
)]
pub(crate) fn references(refs: &[String], options: &Options) -> anyhow::Result<()> {
    let resolver = resolver(options);
    for text in refs {
        println!("{text}");
        let Some(r) = ContentRef::parse(text) else {
            println!("  no content (not a form Roblox loads)");
            continue;
        };
        println!("  parsed:   {r:?}");
        if options.refresh {
            forget(&resolver, &r);
        }
        match resolver.resolve(&r) {
            Ok(f) => print_fetched(&f),
            Err(e) => println!("  failed:   {e}"),
        }
    }
    Ok(())
}
