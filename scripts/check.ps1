# The gate every change must pass: formatting, clippy (warnings are errors), tests.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo clippy --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
# Copied functions, long comment blocks, environment reads below rook-engine
# (apps/rook-cli/src/commands/drift.rs).
cargo run -q -p rook-cli --bin rook-tools -- drift
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
# A file's topics decide when it's split, not its length (see CONTRIBUTING);
# very long ones are pointed out as worth a look. Comments and blank lines
# don't count.
git ls-files '*.rs' '*.wgsl' '*.luau' | ForEach-Object {
    $code = (Get-Content $_ | Where-Object { $_ -match '\S' -and $_ -notmatch '^\s*(//|--)' }).Count
    if ($code -gt 1000) { Write-Warning "$_ has $code lines of code; check it still covers one topic" }
}
Write-Host "check passed"
