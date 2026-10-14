#!/usr/bin/env sh
# The gate every change must pass: formatting, clippy (warnings are errors), tests.
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# Files past about 600 lines should be split by topic (see CONTRIBUTING).
git ls-files '*.rs' '*.wgsl' '*.luau' | while read -r f; do
    n=$(wc -l < "$f")
    if [ "$n" -gt 600 ]; then echo "warning: $f has $n lines; split it by topic"; fi
done
echo "check passed"
