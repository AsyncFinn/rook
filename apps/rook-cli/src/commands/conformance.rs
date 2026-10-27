//! `rook-tools conformance`: record Roblox's behavior in Studio, then check Rook
//! against it.
//!
//! Cases are Luau files under `conformance/cases/<group>/<name>.luau`. A case
//! calls `log(...)` to record lines and may yield. `record` runs every case in
//! a Studio playtest server through Studio's MCP server and writes the output
//! to `conformance/expected/<group>/<name>.txt`. `run` runs the same cases in
//! Rook and compares. Each expected file starts with the Studio version and
//! the signal mode the place used, and Rook runs the case in that mode.
//!
//! A case whose source has a `--! client input ...` line runs on the
//! playtest client instead, with the mouse and keyboard steps it lists
//! played while it runs (input only reaches clients; see [`client`]).
//! A server case with a `--! player` line needs the player a Studio
//! playtest has; Rook joins one for it.

mod client;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use rook_engine::session::Session;
use rook_engine::{DataModel, Options, Server};

/// Wraps a case. Both sides run exactly this code; Rook's copy reports the
/// result through `print` with a marker instead of returning it.
fn wrap(case: &str) -> String {
    format!(
        r##"local __out = {{}}
local function log(...)
    local n = select("#", ...)
    local parts = table.create(n)
    for i = 1, n do
        parts[i] = tostring((select(i, ...)))
    end
    table.insert(__out, table.concat(parts, " "))
end
local __probe = Instance.new("BindableEvent")
local __immediate = false
__probe.Event:Connect(function() __immediate = true end)
__probe:Fire()
local __mode = if __immediate then "immediate" else "deferred"
local __ok, __err = pcall(function()
{case}
end)
if not __ok then
    log("ERROR", __err)
end
return "-- studio " .. version() .. "\n-- signals: " .. __mode .. "\n" .. table.concat(__out, "\n")
"##
    )
}

const MARKER: &str = "\u{1}ROOK-CONFORMANCE\u{1}";

struct Case {
    /// `group/name`.
    id: String,
    source: String,
    /// For client cases, the input played while the case runs.
    input: Option<Vec<client::Step>>,
    /// It needs a player on the server, as a Studio playtest has.
    player: bool,
}

fn cases(root: &Path, filter: Option<&str>) -> anyhow::Result<Vec<Case>> {
    let dir = root.join("cases");
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(&dir).sort_by_file_name() {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("luau") {
            continue;
        }
        let id = path
            .strip_prefix(&dir)?
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");
        if filter.is_some_and(|f| !id.contains(f)) {
            continue;
        }
        let source = fs_err::read_to_string(path)?;
        let input = client::steps(&source)?;
        let player = source.lines().any(|l| l.trim() == "--! player");
        out.push(Case {
            id,
            source,
            input,
            player,
        });
    }
    anyhow::ensure!(!out.is_empty(), "no cases found under {}", dir.display());
    Ok(out)
}

fn expected_path(root: &Path, id: &str) -> PathBuf {
    root.join("expected").join(format!("{id}.txt"))
}

/// Run every case in a Studio playtest and save the results.
pub(crate) fn record(root: &Path, filter: Option<&str>) -> anyhow::Result<()> {
    let cases = cases(root, filter)?;
    let mut studio = super::studio::Studio::connect()?;
    let was_playing = studio.mode()? != "Edit";
    if !was_playing {
        eprintln!("starting a playtest");
        studio.set_playing(true)?;
        std::thread::sleep(Duration::from_secs(3));
    }
    let mut failures = 0;
    for case in &cases {
        let result = match &case.input {
            Some(steps) => studio.run_on_client(&wrap(&case.source), &client::studio_calls(steps)),
            None => studio.run_on_server(&wrap(&case.source)),
        };
        match result {
            Ok(output) => {
                let path = expected_path(root, &case.id);
                std::fs::create_dir_all(path.parent().expect("has parent"))?;
                std::fs::write(&path, format!("{}\n", output.trim_end()))?;
                eprintln!("recorded {}", case.id);
            }
            Err(e) => {
                failures += 1;
                eprintln!("FAILED {}: {e}", case.id);
            }
        }
    }
    if !was_playing {
        studio.set_playing(false)?;
    }
    anyhow::ensure!(failures == 0, "{failures} cases failed to record");
    Ok(())
}

/// Run a case in Rook and return its output. Client cases run on a
/// player's client in a session, as in a Studio playtest.
fn run_in_rook(case: &Case, deferred: bool) -> anyhow::Result<String> {
    let behavior = if deferred {
        rook_lua::SignalBehavior::Deferred
    } else {
        rook_lua::SignalBehavior::Immediate
    };
    let server = Server::new(&Options::default())?;
    server.runtime().world_mut().signal_behavior = behavior;
    let mut session = Session::new(server, Options::default());
    if case.player && case.input.is_none() {
        session.join("Player1", 1)?;
    }
    let client = match case.input {
        Some(_) => {
            let index = session.join("Player1", 1)?;
            let client = session
                .client_mut(index)
                .context("the client didn't join")?;
            client.set_viewport(800, 600);
            client.runtime().world_mut().signal_behavior = behavior;
            Some(index)
        }
        None => None,
    };
    let source = wrap(&case.source);
    let source = match source.rfind("return \"-- studio") {
        Some(i) => format!(
            "{}print(\"{MARKER}\" .. {})",
            &source[..i],
            &source[i + 7..]
        ),
        None => source,
    };
    runs(&mut session, client)
        .runtime()
        .run_source("ServerScriptService.Conformance", &source, None)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    // Client cases get one input step every 12 frames (0.2 s), as in
    // Studio, from 0.2 s in, once their GUI is laid out.
    let mut steps = case.input.iter().flatten();
    let mut log = Vec::new();
    for frame in 0..60 * 60 {
        if frame % 12 == 0
            && frame > 0
            && let (Some(index), Some(step)) = (client, steps.next())
        {
            for event in client::events(step) {
                session.input(index, event);
            }
        }
        log.extend(runs(&mut session, client).take_log());
        if let Some(entry) = log.iter().find(|e| e.message.starts_with(MARKER)) {
            return Ok(entry.message[MARKER.len()..].to_owned());
        }
        session.step();
    }
    let errors: Vec<_> = log.iter().map(|e| e.message.clone()).collect();
    anyhow::bail!("case did not finish within 60 s; output: {errors:?}")
}

/// The DataModel a case runs in: the joined client's, or the server's.
fn runs(session: &mut Session, client: Option<usize>) -> &mut DataModel {
    match client {
        Some(index) => session.client_mut(index).expect("joined client"),
        None => session.server_mut(),
    }
}

/// Run every case that has an expectation in Rook and compare.
pub(crate) fn run(root: &Path, filter: Option<&str>) -> anyhow::Result<()> {
    let cases = cases(root, filter)?;
    let (mut passed, mut failed, mut missing) = (0, 0, 0);
    for case in &cases {
        let Ok(expected) = std::fs::read_to_string(expected_path(root, &case.id)) else {
            missing += 1;
            eprintln!(
                "  ?  {} (no expectation; run `rook-tools conformance record`)",
                case.id
            );
            continue;
        };
        let deferred = expected.lines().any(|l| l.trim() == "-- signals: deferred");
        let actual = match run_in_rook(case, deferred) {
            Ok(a) => a,
            Err(e) => format!("ROOK FAILED: {e}"),
        };
        // The Studio version line is informational; Rook reports its own API version.
        let body = |s: &str| -> Vec<String> {
            s.lines()
                .filter(|l| !l.starts_with("-- studio "))
                .map(|l| l.trim_end().to_owned())
                .collect()
        };
        let (want, got) = (body(&expected), body(&actual));
        if want == got {
            passed += 1;
            println!("  ok {}", case.id);
            continue;
        }
        failed += 1;
        println!("FAIL {}", case.id);
        for i in 0..want.len().max(got.len()) {
            let w = want.get(i).map_or("", String::as_str);
            let g = got.get(i).map_or("", String::as_str);
            if w == g {
                println!("       {w}");
            } else {
                println!("     - {w}");
                println!("     + {g}");
            }
        }
    }
    println!("\n{passed} passed, {failed} failed, {missing} without expectations");
    anyhow::ensure!(failed == 0, "{failed} conformance cases failed");
    Ok(())
}
