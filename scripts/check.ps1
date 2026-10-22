# The gate every change must pass: formatting, clippy (warnings are errors),
# tests, and cargo-deny's bans (`cargo install cargo-deny`).
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo clippy --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
# Only the allowed crates depend on winit, cpal, ureq and rayon (deny.toml).
cargo deny check bans
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
# Code drift (copied functions, long comment blocks, environment reads
# below rook-engine), docs drift (links, indexes, Studio check dates, the
# roadmap's marks) and British spellings (apps/rook-cli/src/commands/drift.rs).
# Built with `--workspace`, not run with `-p rook-cli`: one package alone
# resolves its dependencies' features differently, which compiled about a
# hundred of them again.
cargo build -q --workspace --bin rook-tools
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$target = (cargo metadata -q --format-version 1 --no-deps | ConvertFrom-Json).target_directory
& (Join-Path $target "debug/rook-tools") drift
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
# A file's topics decide when it's split, not its length (see CONTRIBUTING);
# very long ones are pointed out as worth a look. Comments and blank lines
# don't count.
git ls-files '*.rs' '*.wgsl' '*.luau' | ForEach-Object {
    $code = (Get-Content $_ | Where-Object { $_ -match '\S' -and $_ -notmatch '^\s*(//|--)' }).Count
    if ($code -gt 1000) { Write-Warning "$_ has $code lines of code; check it still covers one topic" }
}
Write-Host "check passed"
