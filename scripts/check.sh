#!/usr/bin/env sh
# The gate every change must pass: formatting, clippy (warnings are errors), tests.
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# Copied functions, long comment blocks, environment reads below rook-engine
# (apps/rook-cli/src/commands/drift.rs).
cargo run -q -p rook-cli --bin rook-tools -- drift
# A file's topics decide when it's split, not its length (see CONTRIBUTING);
# very long ones are pointed out as worth a look. Comments and blank lines
# don't count.
git ls-files '*.rs' '*.wgsl' '*.luau' | while read -r f; do
    n=$(grep -cvE '^[[:space:]]*($|//|--)' "$f" || true)
    if [ "$n" -gt 1000 ]; then echo "warning: $f has $n lines of code; check it still covers one topic"; fi
done
echo "check passed"
