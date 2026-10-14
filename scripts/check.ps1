# The gate every change must pass: formatting, clippy (warnings are errors), tests.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo clippy --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
# Files past about 600 lines should be split by topic (see CONTRIBUTING).
git ls-files '*.rs' '*.wgsl' '*.luau' | ForEach-Object {
    $lines = (Get-Content $_ | Measure-Object -Line).Lines
    if ($lines -gt 600) { Write-Warning "$_ has $lines lines; split it by topic" }
}
Write-Host "check passed"
