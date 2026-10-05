#!/usr/bin/env sh
# The gate every change must pass: formatting, clippy (warnings are errors), tests.
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
echo "check passed"
