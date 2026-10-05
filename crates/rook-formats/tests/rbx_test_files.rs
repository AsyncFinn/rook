//! Round-trip tests against rojo-rbx/rbx-test-files (MIT).
//!
//! The fixtures are not committed. Clone them into `fixtures/rbx-test-files`
//! at the workspace root, or point `ROOK_RBX_TEST_FILES` at a checkout:
//!
//!     git clone --depth 1 https://github.com/rojo-rbx/rbx-test-files fixtures/rbx-test-files
//!
//! Without the fixtures these tests pass trivially and say so.

use std::fs;
use std::path::{Path, PathBuf};

use rook_formats::{CompareOptions, Format, compare, compare_with, load, save};

fn fixtures_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("ROOK_RBX_TEST_FILES").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rbx-test-files"),
        PathBuf::from,
    );
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("rbx-test-files not found at {}; skipping", dir.display());
        None
    }
}

fn fixture_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("rbxm" | "rbxl" | "rbxmx" | "rbxlx")
            ) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Every fixture loads, saves in its own format, and reloads unchanged.
#[test]
fn every_fixture_round_trips() {
    let Some(dir) = fixtures_dir() else { return };
    let files = fixture_files(&dir);
    assert!(!files.is_empty(), "no fixtures found in {}", dir.display());

    let mut failures = Vec::new();
    for path in &files {
        let rel = path.strip_prefix(&dir).unwrap().display().to_string();
        let bytes = fs::read(path).unwrap();
        let first = match load(&bytes) {
            Ok(l) => l,
            Err(e) => {
                failures.push(format!("{rel}: load: {e}"));
                continue;
            }
        };
        let saved = match save(&first.dom, first.format) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!("{rel}: save: {e}"));
                continue;
            }
        };
        let second = match load(&saved) {
            Ok(l) => l,
            Err(e) => {
                failures.push(format!("{rel}: reload: {e}"));
                continue;
            }
        };
        let diffs = compare(&first.dom, &second.dom);
        if !diffs.is_empty() {
            failures.push(format!(
                "{rel}: {} differences, first: {:?}",
                diffs.len(),
                diffs[0]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} fixtures failed:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}

/// Pairs whose binary and XML copies are genuinely different files, so a
/// cross-format comparison can't be expected to match. Each has a reason.
const NOT_EQUIVALENT_PAIRS: &[(&str, &str)] = &[
    (
        "models/default-inserted-part",
        "the two copies were inserted separately: different CFrames",
    ),
    (
        "places/",
        "Studio saved a different set of services in each format",
    ),
];

/// The binary and XML copies of each case decode to the same tree, allowing
/// for XML's shorter float formatting.
#[test]
fn binary_and_xml_agree() {
    let Some(dir) = fixtures_dir() else { return };
    let mut compared = 0;
    let mut failures = Vec::new();
    for path in fixture_files(&dir) {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let xml_ext = match ext {
            "rbxm" => "rbxmx",
            "rbxl" => "rbxlx",
            _ => continue,
        };
        let xml_path = path.with_file_name(format!("xml.{xml_ext}"));
        if path.file_stem().and_then(|s| s.to_str()) != Some("binary") || !xml_path.exists() {
            continue;
        }
        let rel = path
            .parent()
            .unwrap()
            .strip_prefix(&dir)
            .unwrap()
            .display()
            .to_string()
            .replace('\\', "/");
        if NOT_EQUIVALENT_PAIRS
            .iter()
            .any(|(prefix, _)| rel.starts_with(prefix))
        {
            continue;
        }
        let (Ok(bin), Ok(xml)) = (
            load(&fs::read(&path).unwrap()),
            load(&fs::read(&xml_path).unwrap()),
        ) else {
            failures.push(format!("{rel}: one of the pair failed to load"));
            continue;
        };
        assert_eq!(bin.format, Format::Binary);
        assert_eq!(xml.format, Format::Xml);
        compared += 1;
        let diffs = compare_with(
            &bin.dom,
            &xml.dom,
            CompareOptions {
                float_tolerance: Some(1e-5),
            },
        );
        if !diffs.is_empty() {
            failures.push(format!(
                "{rel}: {} differences: {:?}",
                diffs.len(),
                &diffs[..diffs.len().min(3)]
            ));
        }
    }
    eprintln!("compared {compared} binary/XML pairs");
    assert!(
        failures.is_empty(),
        "{} pairs differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Every fixture converts into Rook's runtime DOM and back unchanged.
#[test]
fn every_fixture_survives_the_runtime_dom() {
    let Some(dir) = fixtures_dir() else { return };
    let mut failures = Vec::new();
    for path in fixture_files(&dir) {
        let rel = path.strip_prefix(&dir).unwrap().display().to_string();
        let Ok(loaded) = load(&fs::read(&path).unwrap()) else {
            continue;
        };
        let mut dom = rook_dom::Dom::new();
        let db = dom.database();
        let is_place = loaded.dom.root().children().iter().all(|r| {
            let class = &loaded.dom.get_by_ref(*r).unwrap().class;
            db.class_by_name(class).is_some_and(|c| c.flags.service)
        });
        let parent = if is_place {
            dom.root()
        } else {
            let folder = dom.create_by_name("Folder").unwrap();
            let root = dom.root();
            dom.attach_unchecked(folder, root);
            folder
        };
        let top = rook_formats::into_dom(&loaded.dom, &mut dom, parent);
        let back = rook_formats::from_dom(&dom, &top);
        let diffs = compare(&loaded.dom, &back);
        if !diffs.is_empty() {
            failures.push(format!(
                "{rel}: {} differences, first: {:?}",
                diffs.len(),
                diffs[0]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
