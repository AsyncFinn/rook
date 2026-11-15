//! `cargo xtask conformance run`: every case with an expectation in Rook, compared.

use anyhow::Context;
use rook_engine::session::Session;
use rook_engine::{DataModel, Options, Server};

use super::*;

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
                "  ?  {} (no expectation; run `cargo xtask conformance record`)",
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
