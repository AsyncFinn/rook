//! `cargo xtask conformance record`: every case in a Studio playtest.

use std::time::Duration;

use super::*;

/// Run every case in a Studio playtest and save the results.
pub(crate) fn record(root: &Path, filter: Option<&str>) -> anyhow::Result<()> {
    let cases = cases(root, filter)?;
    let mut studio = crate::commands::studio::Studio::connect()?;
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
