//! `raven corpus-scan`: measure what a folder of places and models uses.

mod report;
mod scripts;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::Context;
use rayon::prelude::*;
use rbx_dom_weak::WeakDom;
use rbx_types::Variant;
use serde::Serialize;

use scripts::ScriptUsage;

const EXTENSIONS: [&str; 4] = ["rbxl", "rbxlx", "rbxm", "rbxmx"];

/// Usage of one name across the corpus.
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Usage {
    /// Number of unique files using it.
    pub files: u64,
    /// Total occurrences across all unique files.
    pub count: u64,
}

pub type UsageMap = BTreeMap<String, Usage>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Loaded,
    Duplicate,
    Failed { kind: String, message: String },
}

#[derive(Debug, Serialize)]
pub struct FileRecord {
    pub path: PathBuf,
    pub bytes: u64,
    pub hash: String,
    pub status: Status,
    pub format: Option<String>,
    pub instances: u64,
    pub scripts: u64,
}

/// Everything learned from one unique, successfully loaded file.
#[derive(Debug, Default)]
struct FileSummary {
    classes: BTreeMap<String, u64>,
    unknown_classes: BTreeMap<String, u64>,
    unknown_properties: BTreeMap<String, u64>,
    script_classes: BTreeMap<String, u64>,
    script_bytes: u64,
    script_parse_errors: u64,
    scripts_with_errors: u64,
    usage: ScriptUsage,
}

/// Corpus-wide totals.
#[derive(Debug, Default, Serialize)]
pub struct Aggregate {
    pub files_found: u64,
    pub files_unique: u64,
    pub files_duplicate: u64,
    pub files_loaded: u64,
    pub files_failed: u64,
    pub bytes_unique: u64,
    pub instances: u64,
    pub scripts: u64,
    pub script_bytes: u64,
    pub scripts_with_parse_errors: u64,
    pub formats: BTreeMap<String, u64>,
    pub failures_by_kind: BTreeMap<String, u64>,
    pub failure_messages: UsageMap,
    pub classes: UsageMap,
    pub unknown_classes: UsageMap,
    pub unknown_properties: UsageMap,
    pub script_classes: UsageMap,
    pub methods: UsageMap,
    pub global_calls: UsageMap,
    pub library_calls: UsageMap,
    pub instance_new: UsageMap,
    pub get_service: UsageMap,
    pub game_index: UsageMap,
}

pub fn run(dir: &Path, out: &Path, jobs: Option<usize>, large_mb: u64) -> anyhow::Result<()> {
    anyhow::ensure!(dir.is_dir(), "{} is not a folder", dir.display());
    let started = Instant::now();

    let mut candidates: Vec<(PathBuf, u64)> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| EXTENSIONS.iter().any(|ext| x.eq_ignore_ascii_case(ext)))
        })
        .map(|e| {
            let len = e.metadata().map(|m| m.len()).unwrap_or(0);
            (e.into_path(), len)
        })
        .collect();
    candidates.sort();
    eprintln!("found {} place/model files", candidates.len());

    let large_threshold = large_mb * 1024 * 1024;
    let (large, small): (Vec<_>, Vec<_>) = candidates
        .iter()
        .partition(|(_, len)| *len > large_threshold);

    // Decoder panics are caught and recorded per file; keep them off stderr.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let seen = Mutex::new(HashSet::new());
    let done = AtomicUsize::new(0);
    let total = candidates.len();
    let process = |(path, len): &(PathBuf, u64)| {
        let outcome = process_file(path, *len, &seen);
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        if n % 250 == 0 || n == total {
            eprintln!("  {n}/{total} ({:.0}s)", started.elapsed().as_secs_f32());
        }
        outcome
    };

    let threads =
        jobs.unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get().min(8)));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()?;
    let mut outcomes: Vec<(FileRecord, Option<FileSummary>)> =
        pool.install(|| small.par_iter().map(|c| process(c)).collect());
    eprintln!("processing {} large files one at a time", large.len());
    outcomes.extend(large.iter().map(|c| process(c)));

    std::panic::set_hook(default_hook);

    let mut aggregate = Aggregate {
        files_found: total as u64,
        ..Aggregate::default()
    };
    let mut records = Vec::with_capacity(outcomes.len());
    for (record, summary) in outcomes {
        merge_record(&mut aggregate, &record);
        if let Some(summary) = summary {
            merge_summary(&mut aggregate, summary);
        }
        records.push(record);
    }
    records.sort_by(|a, b| a.path.cmp(&b.path));

    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let db = raven_reflection::database();
    let json = serde_json::json!({
        "generated_by": format!("raven corpus-scan {}", env!("CARGO_PKG_VERSION")),
        "reflection_database_version": db.version,
        "scanned_folder": dir,
        "elapsed_seconds": started.elapsed().as_secs_f64(),
        "aggregate": aggregate,
        "files": records,
    });
    let json_path = out.join("corpus.json");
    std::fs::write(&json_path, serde_json::to_vec_pretty(&json)?)?;
    let summary_path = out.join("summary.md");
    std::fs::write(
        &summary_path,
        report::summary_markdown(&aggregate, &records, started.elapsed()),
    )?;

    eprintln!(
        "done in {:.0}s: {} unique, {} loaded, {} failed, {} duplicates",
        started.elapsed().as_secs_f32(),
        aggregate.files_unique,
        aggregate.files_loaded,
        aggregate.files_failed,
        aggregate.files_duplicate
    );
    eprintln!(
        "wrote {} and {}",
        json_path.display(),
        summary_path.display()
    );
    Ok(())
}

fn process_file(
    path: &Path,
    len: u64,
    seen: &Mutex<HashSet<[u8; 32]>>,
) -> (FileRecord, Option<FileSummary>) {
    let mut record = FileRecord {
        path: path.to_path_buf(),
        bytes: len,
        hash: String::new(),
        status: Status::Loaded,
        format: None,
        instances: 0,
        scripts: 0,
    };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            record.status = Status::Failed {
                kind: "io".into(),
                message: e.to_string(),
            };
            return (record, None);
        }
    };
    let hash = blake3::hash(&bytes);
    record.hash = hash.to_hex().to_string();
    if !seen.lock().expect("seen-set lock").insert(*hash.as_bytes()) {
        record.status = Status::Duplicate;
        return (record, None);
    }

    let loaded = match raven_formats::load(&bytes) {
        Ok(l) => l,
        Err(e) => {
            let kind = match &e {
                raven_formats::LoadError::UnknownFormat => "unknown-format",
                raven_formats::LoadError::Binary(_) => "binary",
                raven_formats::LoadError::Xml(_) => "xml",
                raven_formats::LoadError::Panicked(_) => "panic",
            };
            record.status = Status::Failed {
                kind: kind.into(),
                message: e.to_string(),
            };
            return (record, None);
        }
    };
    drop(bytes);
    record.format = Some(format!("{:?}", loaded.format));
    let summary = summarise(&loaded.dom);
    record.instances = summary.classes.values().sum();
    record.scripts = summary.script_classes.values().sum();
    (record, Some(summary))
}

fn summarise(dom: &WeakDom) -> FileSummary {
    let db = raven_reflection::database();
    let source_container = db.class_by_name("LuaSourceContainer").map(|c| c.id);
    let mut s = FileSummary::default();
    for inst in dom.descendants().skip(1) {
        let class = inst.class.as_str();
        *s.classes.entry(class.to_owned()).or_insert(0) += 1;
        let Some(info) = db.class_by_name(class) else {
            *s.unknown_classes.entry(class.to_owned()).or_insert(0) += 1;
            continue;
        };
        for key in inst.properties.keys() {
            if db.find_property(info.id, key.as_str()).is_none() {
                *s.unknown_properties
                    .entry(format!("{class}.{key}"))
                    .or_insert(0) += 1;
            }
        }
        if source_container.is_some_and(|sc| db.is_a(info.id, sc)) {
            *s.script_classes.entry(class.to_owned()).or_insert(0) += 1;
            if let Some(Variant::String(source)) = inst.properties.get(&"Source".into()) {
                s.script_bytes += source.len() as u64;
                let analysis = scripts::analyse(source);
                if analysis.parse_errors > 0 {
                    s.scripts_with_errors += 1;
                    s.script_parse_errors += analysis.parse_errors as u64;
                }
                merge_usage(&mut s.usage, analysis.usage);
            }
        }
    }
    s
}

fn merge_usage(into: &mut ScriptUsage, from: ScriptUsage) {
    for (dst, src) in [
        (&mut into.methods, from.methods),
        (&mut into.global_calls, from.global_calls),
        (&mut into.library_calls, from.library_calls),
        (&mut into.instance_new, from.instance_new),
        (&mut into.get_service, from.get_service),
        (&mut into.game_index, from.game_index),
    ] {
        for (k, v) in src {
            *dst.entry(k).or_insert(0) += v;
        }
    }
}

fn merge_record(agg: &mut Aggregate, record: &FileRecord) {
    match &record.status {
        Status::Duplicate => agg.files_duplicate += 1,
        Status::Loaded => {
            agg.files_unique += 1;
            agg.files_loaded += 1;
            agg.bytes_unique += record.bytes;
            agg.instances += record.instances;
            agg.scripts += record.scripts;
            if let Some(f) = &record.format {
                *agg.formats.entry(f.clone()).or_insert(0) += 1;
            }
        }
        Status::Failed { kind, message } => {
            agg.files_unique += 1;
            agg.files_failed += 1;
            agg.bytes_unique += record.bytes;
            *agg.failures_by_kind.entry(kind.clone()).or_insert(0) += 1;
            let usage = agg
                .failure_messages
                .entry(normalise_message(message))
                .or_default();
            usage.files += 1;
            usage.count += 1;
        }
    }
}

fn merge_summary(agg: &mut Aggregate, s: FileSummary) {
    agg.script_bytes += s.script_bytes;
    agg.scripts_with_parse_errors += s.scripts_with_errors;
    for (dst, src) in [
        (&mut agg.classes, s.classes),
        (&mut agg.unknown_classes, s.unknown_classes),
        (&mut agg.unknown_properties, s.unknown_properties),
        (&mut agg.script_classes, s.script_classes),
        (&mut agg.methods, s.usage.methods),
        (&mut agg.global_calls, s.usage.global_calls),
        (&mut agg.library_calls, s.usage.library_calls),
        (&mut agg.instance_new, s.usage.instance_new),
        (&mut agg.get_service, s.usage.get_service),
        (&mut agg.game_index, s.usage.game_index),
    ] {
        for (name, count) in src {
            let u = dst.entry(name).or_default();
            u.files += 1;
            u.count += count;
        }
    }
}

/// Group similar error messages: digits become `#`, long messages are cut.
fn normalise_message(message: &str) -> String {
    let mut out = String::with_capacity(message.len().min(160));
    let mut last_digit = false;
    for ch in message.chars().take(160) {
        if ch.is_ascii_digit() {
            if !last_digit {
                out.push('#');
            }
            last_digit = true;
        } else {
            out.push(ch);
            last_digit = false;
        }
    }
    out
}
