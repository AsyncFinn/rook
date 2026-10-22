#!/usr/bin/env sh
# The gate every change must pass: formatting, clippy (warnings are errors),
# tests, and cargo-deny's bans (`cargo install cargo-deny`).
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# Only the allowed crates depend on winit, cpal, ureq and rayon (deny.toml).
cargo deny check bans
# Code drift (copied functions, long comment blocks, environment reads
# below rook-engine), docs drift (links, indexes, Studio check dates, the
# roadmap's marks) (apps/rook-cli/src/commands/drift.rs).
# Built with `--workspace`, not run with `-p rook-cli`: one package alone
# resolves its dependencies' features differently, which compiled about a
# hundred of them again.
cargo build -q --workspace --bin rook-tools
"${CARGO_TARGET_DIR:-target}/debug/rook-tools" drift
# A file's topics decide when it's split, not its length (see CONTRIBUTING);
# very long ones are pointed out as worth a look. Comments and blank lines
# don't count.
git ls-files '*.rs' '*.wgsl' '*.luau' | while read -r f; do
    n=$(grep -cvE '^[[:space:]]*($|//|--)' "$f" || true)
    if [ "$n" -gt 1000 ]; then echo "warning: $f has $n lines of code; check it still covers one topic"; fi
done
echo "check passed"
