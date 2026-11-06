//! Human-readable summary of a corpus scan.

use std::fmt::Write as _;
use std::time::Duration;

use super::{Aggregate, FileRecord, Status, UsageMap};

pub(super) fn summary_markdown(
    agg: &Aggregate,
    records: &[FileRecord],
    elapsed: Duration,
) -> String {
    let mut md = String::new();
    overview(&mut md, agg, elapsed);
    legacy_upgrades(&mut md, agg);
    failures(&mut md, agg, records);
    classes(&mut md, agg);
    content(&mut md, agg);
    md
}

fn pct(n: u64, of: u64) -> f64 {
    if of == 0 {
        0.0
    } else {
        n as f64 * 100.0 / of as f64
    }
}

/// The scan's totals.
fn overview(md: &mut String, agg: &Aggregate, elapsed: Duration) {
    let db = rook_reflection::database();
    let _ = writeln!(md, "# Corpus scan\n");
    let _ = writeln!(
        md,
        "Reflection database {:?}. Scanned in {:.0}s.\n",
        db.version,
        elapsed.as_secs_f64()
    );
    let _ = writeln!(md, "| | |\n|---|---|");
    let _ = writeln!(md, "| Files found | {} |", agg.files_found);
    let _ = writeln!(md, "| Duplicates skipped | {} |", agg.files_duplicate);
    let _ = writeln!(
        md,
        "| Unique files | {} ({:.1} GB) |",
        agg.files_unique,
        agg.bytes_unique as f64 / 1e9
    );
    let _ = writeln!(
        md,
        "| **Loaded** | **{} ({:.1}%)** |",
        agg.files_loaded,
        pct(agg.files_loaded, agg.files_unique)
    );
    let _ = writeln!(md, "| Failed | {} |", agg.files_failed);
    for (format, n) in &agg.formats {
        let _ = writeln!(md, "| Loaded as {format} | {n} |");
    }
    let _ = writeln!(md, "| Instances | {} |", agg.instances);
    let _ = writeln!(
        md,
        "| Scripts | {} ({:.1} MB of source) |",
        agg.scripts,
        agg.script_bytes as f64 / 1e6
    );
    let _ = writeln!(
        md,
        "| Scripts with syntax errors | {} ({:.1}%) |\n",
        agg.scripts_with_parse_errors,
        pct(agg.scripts_with_parse_errors, agg.scripts)
    );
}

/// What upgrading legacy XML files changed.
fn legacy_upgrades(md: &mut String, agg: &Aggregate) {
    if agg.legacy_upgraded_files > 0 {
        let t = &agg.legacy_totals;
        let _ = writeln!(
            md,
            "## Legacy XML upgrade
"
        );
        let _ = writeln!(
            md,
            "{} files needed the legacy XML upgrade to load.
",
            agg.legacy_upgraded_files
        );
        let _ = writeln!(
            md,
            "| Upgrade | Total |
|---|---:|"
        );
        let _ = writeln!(
            md,
            "| `<External>` entries dropped | {} |",
            t.externals_dropped
        );
        let _ = writeln!(
            md,
            "| Embedded assets (loaded empty, as in Studio) | {} ({:.1} MB) |",
            t.embedded_assets,
            t.embedded_bytes as f64 / 1e6
        );
        let _ = writeln!(
            md,
            "| `<hash>` references (loaded empty) | {} |",
            t.hash_references
        );
        let _ = writeln!(
            md,
            "| Control characters restored | {} |",
            t.control_characters
        );
        let _ = writeln!(
            md,
            "| MSVC float values (`1.#QNAN`, ...) | {} |",
            t.msvc_floats
        );
        let _ = writeln!(
            md,
            "| `null` numbers read as 0 (unverified) | {} |
",
            t.null_numbers
        );
    }
}

/// Why files failed to load, with examples.
fn failures(md: &mut String, agg: &Aggregate, records: &[FileRecord]) {
    if agg.files_failed > 0 {
        let _ = writeln!(md, "## Load failures\n");
        for (kind, n) in &agg.failures_by_kind {
            let _ = writeln!(md, "- **{kind}**: {n}");
        }
        let _ = writeln!(md);
        usage_table(
            md,
            "Most common failure messages",
            "Message",
            &agg.failure_messages,
            25,
        );
        let _ = writeln!(md, "Example failing files:\n");
        for r in records
            .iter()
            .filter(|r| matches!(r.status, Status::Failed { .. }))
            .take(20)
        {
            if let Status::Failed { kind, .. } = &r.status {
                let name = r
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default();
                let _ = writeln!(md, "- `{name}` ({kind}, {} KB)", r.bytes / 1024);
            }
        }
        let _ = writeln!(md);
    }
}

/// Classes, properties and script calls in use.
fn classes(md: &mut String, agg: &Aggregate) {
    let db = rook_reflection::database();
    // Classes, annotated with database status.
    let _ = writeln!(md, "## Classes by number of files using them\n");
    let _ = writeln!(
        md,
        "| Class | Files | Instances | Notes |\n|---|---:|---:|---|"
    );
    for (name, u) in sorted(&agg.classes).into_iter().take(80) {
        let notes = match db.class_by_name(name) {
            None => "not in database".to_owned(),
            Some(c) if c.flags.deprecated => "deprecated".to_owned(),
            Some(c) if c.flags.service => "service".to_owned(),
            Some(_) => String::new(),
        };
        let _ = writeln!(md, "| {name} | {} | {} | {notes} |", u.files, u.count);
    }
    let _ = writeln!(md);

    let deprecated: UsageMap = agg
        .classes
        .iter()
        .filter(|(name, _)| db.class_by_name(name).is_some_and(|c| c.flags.deprecated))
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    usage_table(md, "Deprecated classes in use", "Class", &deprecated, 50);
    usage_table(
        md,
        "Classes not in the modern database",
        "Class",
        &agg.unknown_classes,
        50,
    );
    usage_table(
        md,
        "Properties not in the modern database",
        "Class.Property",
        &agg.unknown_properties,
        60,
    );
    usage_table(md, "Script classes", "Class", &agg.script_classes, 20);
    usage_table(
        md,
        "Methods called (`obj:Method()`)",
        "Method",
        &agg.methods,
        100,
    );
    usage_table(
        md,
        "Global functions called",
        "Function",
        &agg.global_calls,
        60,
    );
    usage_table(
        md,
        "Library calls (`Lib.func()`)",
        "Call",
        &agg.library_calls,
        60,
    );
    usage_table(
        md,
        "Classes created with `Instance.new`",
        "Class",
        &agg.instance_new,
        60,
    );
    usage_table(
        md,
        "Services requested with `GetService`",
        "Service",
        &agg.get_service,
        40,
    );
    usage_table(
        md,
        "Direct `game.X` accesses",
        "Access",
        &agg.game_index,
        40,
    );
}

/// Content references and their URL shapes.
fn content(md: &mut String, agg: &Aggregate) {
    let _ = writeln!(
        md,
        "## Content references

{} non-empty references ({} distinct values); {} empty.
",
        agg.content_shapes.values().map(|u| u.count).sum::<u64>(),
        agg.content_distinct,
        agg.content_empty
    );
    usage_table(
        md,
        "Content references by kind",
        "Kind",
        &agg.content_kinds,
        20,
    );
    usage_table(md, "Meshes and unions", "Kind", &agg.meshes, 100);
    usage_table(
        md,
        "Materials, surfaces, decals and textures",
        "Kind",
        &agg.looks,
        300,
    );
    usage_table(
        md,
        "`rbxasset://` paths the install lacks",
        "Kind: path",
        &agg.content_paths,
        200,
    );
    usage_table(
        md,
        "Content properties",
        "Class.Property",
        &agg.content_properties,
        60,
    );
    usage_table(md, "Content URL shapes", "Shape", &agg.content_shapes, 80);
    usage_table(
        md,
        "Content URL shapes by property",
        "Property shape",
        &agg.content_by_property,
        120,
    );
    usage_table(
        md,
        "Content URLs in script sources",
        "Shape",
        &agg.content_script_shapes,
        60,
    );
    let _ = writeln!(
        md,
        "## Content URL samples
"
    );
    for (shape, sample) in &agg.content_samples {
        let _ = writeln!(md, "- `{shape}`: `{sample}`");
    }
    let _ = writeln!(md);
}

fn sorted(map: &UsageMap) -> Vec<(&String, &super::Usage)> {
    let mut v: Vec<_> = map.iter().collect();
    v.sort_by(|a, b| {
        b.1.files
            .cmp(&a.1.files)
            .then(b.1.count.cmp(&a.1.count))
            .then(a.0.cmp(b.0))
    });
    v
}

fn usage_table(md: &mut String, title: &str, column: &str, map: &UsageMap, limit: usize) {
    if map.is_empty() {
        return;
    }
    let _ = writeln!(md, "## {title}\n");
    if map.len() > limit {
        let _ = writeln!(md, "Top {limit} of {} by files using them.\n", map.len());
    }
    let _ = writeln!(md, "| {column} | Files | Uses |\n|---|---:|---:|");
    for (name, u) in sorted(map).into_iter().take(limit) {
        let name = name.replace('|', "\\|").replace('\n', " ");
        let _ = writeln!(md, "| `{name}` | {} | {} |", u.files, u.count);
    }
    let _ = writeln!(md);
}
