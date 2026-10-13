//! `rook corpus-run`: run every place in a folder headless and report how
//! many run their server Scripts without errors. This is M1's archive metric.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context;
use rayon::prelude::*;
use rook_engine::{MessageType, Options, Server};
use serde::Serialize;

/// Decoders and the Luau parser recurse deeply on some archive files.
const WORKER_STACK: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    /// Scripts ran for the whole time without an error.
    Clean,
    /// Loaded, but no server Script runs in it.
    NoScripts,
    /// At least one script error.
    Errors,
    LoadFailed,
    /// Rook itself panicked (a Rook bug).
    Crashed,
}

#[derive(Debug, Serialize)]
struct PlaceRecord {
    path: PathBuf,
    outcome: Outcome,
    scripts_started: usize,
    errors: usize,
    /// First line of each distinct error, normalised.
    error_kinds: Vec<String>,
    unimplemented: BTreeMap<String, u64>,
    inert: BTreeMap<String, u64>,
    load_error: Option<String>,
    wall_seconds: f64,
}

pub fn run(
    dir: &Path,
    out: &Path,
    seconds: f64,
    timeout: f64,
    jobs: Option<usize>,
) -> anyhow::Result<()> {
    anyhow::ensure!(dir.is_dir(), "{} is not a folder", dir.display());
    let started = Instant::now();
    let places: Vec<PathBuf> = crate::corpus::find_files(dir, &["rbxl", "rbxlx"])
        .into_iter()
        .map(walkdir::DirEntry::into_path)
        .collect();
    eprintln!("found {} place files", places.len());

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let seen = Mutex::new(HashSet::new());
    let done = AtomicUsize::new(0);
    let total = places.len();
    let options = Options {
        script_timeout: Duration::from_secs_f64(timeout),
        memory_limit: 512 << 20,
        ..Options::default()
    };
    let threads =
        jobs.unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get().min(8)));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .stack_size(WORKER_STACK)
        .build()?;
    let records: Vec<PlaceRecord> = pool.install(|| {
        places
            .par_iter()
            .filter_map(|path| {
                let bytes = std::fs::read(path).ok()?;
                let hash = blake3::hash(&bytes);
                if !seen.lock().expect("not poisoned").insert(hash) {
                    return None;
                }
                let record = run_place(path, &bytes, seconds, &options);
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                if n.is_multiple_of(100) {
                    eprintln!("  {n} places ({:.0}s)", started.elapsed().as_secs_f32());
                }
                Some(record)
            })
            .collect()
    });
    std::panic::set_hook(default_hook);
    let _ = total;

    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let json = serde_json::json!({
        "generated_by": format!("rook corpus-run {}", env!("CARGO_PKG_VERSION")),
        "roblox_version": rook_reflection::roblox_version(),
        "seconds_simulated": seconds,
        "script_timeout": timeout,
        "elapsed_seconds": started.elapsed().as_secs_f64(),
        "places": records,
    });
    std::fs::write(out.join("run.json"), serde_json::to_vec_pretty(&json)?)?;
    let summary = summary_markdown(&records, seconds, started.elapsed());
    std::fs::write(out.join("summary.md"), &summary)?;
    eprintln!(
        "{}",
        summary.lines().take(14).collect::<Vec<_>>().join("\n")
    );
    eprintln!("wrote {}", out.join("summary.md").display());
    Ok(())
}

fn run_place(path: &Path, bytes: &[u8], seconds: f64, options: &Options) -> PlaceRecord {
    let started = Instant::now();
    let mut record = PlaceRecord {
        path: path.to_owned(),
        outcome: Outcome::Crashed,
        scripts_started: 0,
        errors: 0,
        error_kinds: Vec::new(),
        unimplemented: BTreeMap::new(),
        inert: BTreeMap::new(),
        load_error: None,
        wall_seconds: 0.0,
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut server = match Server::load_place(bytes, options) {
            Ok(s) => s,
            Err(e) => return Err(e.to_string()),
        };
        server.start();
        server.run_for(seconds);
        let log = server.take_log();
        let world = server.runtime().world();
        let mut kinds: Vec<String> = log
            .iter()
            .filter(|e| e.kind == MessageType::Error)
            .map(|e| error_kind(&e.message))
            .collect();
        let errors = kinds.len();
        kinds.sort();
        kinds.dedup();
        Ok((
            server.started_scripts(),
            errors,
            kinds,
            world.unimplemented.clone(),
            world.inert.clone(),
        ))
    }));
    match result {
        Ok(Ok((started_scripts, errors, kinds, unimplemented, inert))) => {
            record.scripts_started = started_scripts;
            record.errors = errors;
            record.error_kinds = kinds;
            record.unimplemented = unimplemented;
            record.inert = inert;
            record.outcome = if errors > 0 {
                Outcome::Errors
            } else if started_scripts == 0 {
                Outcome::NoScripts
            } else {
                Outcome::Clean
            };
        }
        Ok(Err(e)) => {
            record.outcome = Outcome::LoadFailed;
            record.load_error = Some(e);
        }
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            record.load_error = Some(format!("rook panicked: {msg}"));
        }
    }
    record.wall_seconds = started.elapsed().as_secs_f64();
    record
}

/// The message without its script location, with digits and quoted names
/// generalised, so the same failure in different scripts groups together.
fn error_kind(message: &str) -> String {
    let first = message.lines().next().unwrap_or("");
    // Drop a leading "Some.Script.Path:12: ".
    let body = match first.split_once(": ") {
        Some((loc, rest))
            if loc
                .rsplit_once(':')
                .is_some_and(|(_, n)| n.parse::<u32>().is_ok()) =>
        {
            rest
        }
        _ => first,
    };
    let mut out = String::new();
    let mut in_quote: Option<char> = None;
    let mut last_digit = false;
    for ch in body.chars().take(200) {
        match in_quote {
            Some(q) if ch == q => {
                out.push(ch);
                in_quote = None;
            }
            Some(_) => {}
            None if ch == '"' || ch == '\'' => {
                out.push(ch);
                out.push('…');
                in_quote = Some(ch);
            }
            None if ch.is_ascii_digit() => {
                if !last_digit {
                    out.push('#');
                }
                last_digit = true;
                continue;
            }
            None => out.push(ch),
        }
        last_digit = false;
    }
    out
}

fn summary_markdown(records: &[PlaceRecord], seconds: f64, elapsed: Duration) -> String {
    let count = |o: Outcome| records.iter().filter(|r| r.outcome == o).count();
    let total = records.len().max(1);
    let pct = |n: usize| 100.0 * n as f64 / total as f64;
    let with_scripts = count(Outcome::Clean) + count(Outcome::Errors);
    let mut s = String::new();
    let _ = writeln!(s, "# Corpus run\n");
    let _ = writeln!(
        s,
        "Every unique place run headless for {seconds} s of game time (server Scripts only), in {:.0} s.\n",
        elapsed.as_secs_f64()
    );
    let _ = writeln!(s, "| Outcome | Places | Share |\n|---|---:|---:|");
    for (label, o) in [
        ("Ran without errors", Outcome::Clean),
        ("Script errors", Outcome::Errors),
        ("No server scripts", Outcome::NoScripts),
        ("Failed to load", Outcome::LoadFailed),
        ("Rook crashed", Outcome::Crashed),
    ] {
        let n = count(o);
        let _ = writeln!(s, "| {label} | {n} | {:.1}% |", pct(n));
    }
    let _ = writeln!(
        s,
        "\n**Headline:** {} of {with_scripts} places with server scripts ran {seconds} s without errors ({:.1}%).\n",
        count(Outcome::Clean),
        100.0 * count(Outcome::Clean) as f64 / with_scripts.max(1) as f64
    );

    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for r in records {
        for k in &r.error_kinds {
            *kinds.entry(k).or_default() += 1;
        }
    }
    let mut kinds: Vec<_> = kinds.into_iter().collect();
    kinds.sort_by_key(|k| std::cmp::Reverse(k.1));
    let _ = writeln!(
        s,
        "## Most common errors (by places)\n\n| Places | Error |\n|---:|---|"
    );
    for (k, n) in kinds.iter().take(60) {
        let _ = writeln!(s, "| {n} | `{}` |", k.replace('|', "\\|").replace('`', "'"));
    }

    let mut members: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    for r in records {
        for (m, calls) in &r.unimplemented {
            let e = members.entry(m).or_default();
            e.0 += 1;
            e.1 += calls;
        }
    }
    let mut members: Vec<_> = members.into_iter().collect();
    members.sort_by_key(|m| std::cmp::Reverse(m.1.0));
    let _ = writeln!(
        s,
        "\n## Members not implemented yet (by places)\n\n| Places | Calls | Member |\n|---:|---:|---|"
    );
    for (m, (places, calls)) in members.iter().take(60) {
        let _ = writeln!(s, "| {places} | {calls} | `{m}` |");
    }

    let mut inert: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    for r in records {
        for (m, calls) in &r.inert {
            let e = inert.entry(m).or_default();
            e.0 += 1;
            e.1 += calls;
        }
    }
    let _ = writeln!(
        s,
        "\n## Accepted but ignored for now\n\n| Places | Calls | Member |\n|---:|---:|---|"
    );
    for (m, (places, calls)) in &inert {
        let _ = writeln!(s, "| {places} | {calls} | `{m}` |");
    }

    let crashed: Vec<_> = records
        .iter()
        .filter(|r| r.outcome == Outcome::Crashed)
        .collect();
    if !crashed.is_empty() {
        let _ = writeln!(s, "\n## Rook crashes\n");
        for r in crashed.iter().take(30) {
            let _ = writeln!(
                s,
                "- {}",
                r.load_error.as_deref().unwrap_or("?").replace('\n', " ")
            );
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::error_kind;

    #[test]
    fn groups_errors() {
        assert_eq!(
            error_kind(
                "Workspace.Model.Script:12: attempt to index nil with 'Humanoid'\nStack Begin"
            ),
            "attempt to index nil with '…'"
        );
        assert_eq!(
            error_kind("Foo is not a valid member of Workspace \"Workspace\""),
            "Foo is not a valid member of Workspace \"…\""
        );
    }
}
