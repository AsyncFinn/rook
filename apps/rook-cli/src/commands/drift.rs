//! `rook-tools drift`: what `scripts/check` runs against the repository
//! drifting from CONTRIBUTING's rules ("Keeping it clean"): the code
//! checks in [`code`], the documentation checks in [`docs`], and American
//! spelling over both in [`spelling`]. Errors fail the check; warnings
//! point at something worth a look.

mod code;
mod docs;
mod source;
mod spelling;

pub(crate) fn run() -> anyhow::Result<()> {
    let mut report = Report::default();
    code::check(&mut report)?;
    docs::check(&mut report)?;
    spelling::check(&mut report)?;
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
    for error in &report.errors {
        println!("error: {error}");
    }
    anyhow::ensure!(
        report.errors.is_empty(),
        "{} drift error(s)",
        report.errors.len()
    );
    Ok(())
}

/// What the checks found.
#[derive(Debug, Default)]
struct Report {
    errors: Vec<String>,
    warnings: Vec<String>,
}

/// The files git tracks that match `patterns`.
fn tracked(patterns: &[&str]) -> anyhow::Result<Vec<String>> {
    let listed = std::process::Command::new("git")
        .arg("ls-files")
        .args(patterns)
        .output()?;
    anyhow::ensure!(listed.status.success(), "git ls-files failed");
    Ok(String::from_utf8(listed.stdout)?
        .lines()
        .map(str::to_owned)
        .collect())
}

#[cfg(test)]
mod tests;
