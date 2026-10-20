use std::path::Path;

use rook_formats::{CompareOptions, Format, compare_with, load, save};

pub(crate) fn run(file: &Path, to: Option<Format>) -> anyhow::Result<()> {
    let bytes = fs_err::read(file)?;
    let first = load(&bytes)?;
    let target = to.unwrap_or(first.format);
    let saved = save(&first.dom, target)?;
    let second = load(&saved)?;

    // Converting to XML loses float digits, so allow a small tolerance then.
    let options = CompareOptions {
        float_tolerance: (first.format != target).then_some(1e-5),
    };
    let diffs = compare_with(&first.dom, &second.dom, options);
    println!(
        "{}: {:?} -> {:?}, {} bytes -> {} bytes",
        file.display(),
        first.format,
        target,
        bytes.len(),
        saved.len()
    );
    if diffs.is_empty() {
        println!("round trip OK: no differences");
        return Ok(());
    }
    for d in diffs.iter().take(50) {
        println!("  {d:?}");
    }
    if diffs.len() > 50 {
        println!("  ... and {} more", diffs.len() - 50);
    }
    anyhow::bail!("{} differences", diffs.len())
}
