//! Human-readable summary of a corpus scan.

use std::fmt::Write as _;
use std::time::Duration;

use super::{Aggregate, FileRecord, Status, UsageMap};

pub fn summary_markdown(agg: &Aggregate, records: &[FileRecord], elapsed: Duration) -> String {
    let db = raven_reflection::database();
    let mut md = String::new();
    let pct = |n: u64, of: u64| {
        if of == 0 {
            0.0
        } else {
            n as f64 * 100.0 / of as f64
        }
    };

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

    if agg.files_failed > 0 {
        let _ = writeln!(md, "## Load failures\n");
        for (kind, n) in &agg.failures_by_kind {
            let _ = writeln!(md, "- **{kind}**: {n}");
        }
        let _ = writeln!(md);
        usage_table(
            &mut md,
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
    usage_table(
        &mut md,
        "Deprecated classes in use",
        "Class",
        &deprecated,
        50,
    );
    usage_table(
        &mut md,
        "Classes not in the modern database",
        "Class",
        &agg.unknown_classes,
        50,
    );
    usage_table(
        &mut md,
        "Properties not in the modern database",
        "Class.Property",
        &agg.unknown_properties,
        60,
    );
    usage_table(&mut md, "Script classes", "Class", &agg.script_classes, 20);
    usage_table(
        &mut md,
        "Methods called (`obj:Method()`)",
        "Method",
        &agg.methods,
        100,
    );
    usage_table(
        &mut md,
        "Global functions called",
        "Function",
        &agg.global_calls,
        60,
    );
    usage_table(
        &mut md,
        "Library calls (`Lib.func()`)",
        "Call",
        &agg.library_calls,
        60,
    );
    usage_table(
        &mut md,
        "Classes created with `Instance.new`",
        "Class",
        &agg.instance_new,
        60,
    );
    usage_table(
        &mut md,
        "Services requested with `GetService`",
        "Service",
        &agg.get_service,
        40,
    );
    usage_table(
        &mut md,
        "Direct `game.X` accesses",
        "Access",
        &agg.game_index,
        40,
    );
    md
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
